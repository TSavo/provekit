package com.provekit.realize;

import com.provekit.ir.Blake3;
import com.provekit.ir.Jcs;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.stream.Collectors;

/**
 * Emits canonical Java method bodies and stubs for the bind canonical-rewrite path.
 *
 * Per the federation-by-construction directive ("all java emitter code belongs in java"),
 * this class is the SOLE owner of Java surface emission. cmd_transport.rs dispatches to
 * this class via the JSON-RPC plugin protocol; Rust holds no Java syntax.
 *
 * Output shape (per cmd_transport.rs Java branch, byte-identical):
 * <pre>
 * final class {PascalFn}Transported {
 *     // concept: {conceptName}
 *     public static {returnType} {function}({typedParams}) {
 *         {body}
 *     }
 * }
 * </pre>
 *
 * Where {body} is rendered from the body-template plugin (loaded from
 * classpath resource `com/provekit/realize/java-canonical-bodies.json`,
 * sourced from `menagerie/java-language-signature/specs/body-templates/`)
 * when an entry matches the binding's concept_name + signature_guard.
 * When no entry matches, the language stub falls through:
 *   `throw new UnsupportedOperationException("provekit-bind canonical: <concept>");`
 */
final class SugarRealizer {

    /**
     * Realization result: the emitted Java source plus a flag indicating
     * whether the body came from a body-template (real body, `is_stub=false`)
     * or fell through to the language stub (`is_stub=true`).
     *
     * Carried through to the JSON-RPC response so the caller (cmd_bind) can
     * emit accurate per-concept `bind-stub-body-emitted` gap entries per
     * `2026-05-13-body-template-memento.md` §5.
     */
    record Realization(
            String source,
            boolean isStub,
            String observedLossRecord,
            String usedSugarsJson,
            String observationWrapperEmissionRecord) {}

    /**
     * Emit a Java method for a single function. Body source comes from the
     * body-template plugin when a matching entry exists; otherwise an
     * idiomatic stub is emitted.
     *
     * @param function    Rust snake_case function name (e.g. "wrap_identity")
     * @param params      Parameter names in order (e.g. ["x"])
     * @param paramTypes  Source-language (Rust) type strings (e.g. ["i64"])
     * @param returnType  Source-language return type string (e.g. "i64")
     * @param conceptName Concept binding name (e.g. "identity")
     * @return Realization carrying source + is_stub flag.
     */
    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName) {
        return emitStub(function, params, paramTypes, returnType, conceptName, "", null);
    }

    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName,
            String mode,
            ContractPayload contract) {
        return emitStub(function, params, paramTypes, returnType, conceptName, mode, contract, List.of());
    }

    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName,
            String mode,
            ContractPayload contract,
            List<String> sugarPluginJson) {
        return emitStub(function, params, paramTypes, returnType, conceptName, mode, modeList(mode), contract, sugarPluginJson);
    }

    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName,
            String mode,
            List<String> modes,
            ContractPayload contract,
            List<String> sugarPluginJson) {

        String className = snakeToPascal(function) + "Transported";
        String mappedReturn = mapSourceType(returnType);
        List<String> requestedModes = modeList(mode, modes);
        List<SugarEmission> sugarEmissions = SugarDictionary.emitAll(contract, sugarPluginJson, requestedModes);
        boolean hasBeanValidationNotNull = sugarEmissions.stream()
                .anyMatch(e -> e.surfaceLocator().startsWith("annotation:") && e.rendered().startsWith("@NotNull"));
        boolean hasJUnitWitness = sugarEmissions.stream()
                .anyMatch(e -> e.surfaceLocator().startsWith("witness:junit5"));

        StringBuilder typedParamList = new StringBuilder();
        for (int i = 0; i < params.size(); i++) {
            String name = params.get(i);
            String srcType = i < paramTypes.size() ? paramTypes.get(i) : "i64";
            String mapped = mapSourceType(srcType);
            if (i > 0) typedParamList.append(", ");
            String parameterAnnotations = parameterAnnotations(sugarEmissions, name);
            if (!parameterAnnotations.isBlank()) {
                typedParamList.append(parameterAnnotations).append(" ");
            }
            typedParamList.append(mapped).append(" ").append(name);
        }

        // annotation_prefix for Java: top_indent = "    "
        String annotationPrefix = "    // concept: " + conceptName + "\n"
                + contractPrefix(contract)
                + commentPrefix(contract, sugarEmissions);

        Optional<RenderedBody> bodyTemplate = renderBodyTemplateFor(conceptName, params, mode);
        boolean isStub = bodyTemplate.isEmpty();
        String bodyContent = bodyTemplate.map(RenderedBody::body)
                .orElse("throw new UnsupportedOperationException(\"provekit-bind canonical: " + conceptName + "\");");
        Jcs.Json bodyLossRecord = bodyTemplate.map(RenderedBody::lossRecord).orElse(Jcs.object());
        String observationWrapperEmissionRecord = null;
        if (!isStub) {
            Optional<ObservationComposition> observation = composeEmitterAfterReturn(
                    requestedModes,
                    bodyContent,
                    mappedReturn,
                    function,
                    conceptName,
                    contract);
            if (observation.isPresent()) {
                bodyContent = observation.get().body();
                bodyLossRecord = combineLossRecords(bodyLossRecord, observation.get().lossRecord());
                observationWrapperEmissionRecord = observation.get().observationWrapperEmissionRecord();
            }
        }
        // Multi-line templates: each internal line gets the same 8-space
        // method-body indent. Single-line bodies are unaffected (no \n).
        String indentedBody = bodyContent.replace("\n", "\n        ");
        String body = "        " + indentedBody + "\n";
        String methodAnnotation = hasBeanValidationNotNull && contractHasNonNullPostcondition(contract) ? "    @NotNull\n" : "";
        String imports = importsFor(sugarEmissions, !methodAnnotation.isEmpty(), hasJUnitWitness);
        String witnessClass = hasJUnitWitness
                ? "\nfinal class " + className + "WitnessTest {\n"
                        + "    @Disabled(\"provekit witness skeleton requires concrete values\")\n"
                        + "    @Test\n"
                        + "    void contractWitnessesRemainRecoverable() {\n"
                        + junitWitnessBody(sugarEmissions)
                        + "    }\n"
                        + "}\n"
                : "";

        String source = imports
                + "final class " + className + " {\n"
                + annotationPrefix
                + methodAnnotation
                + "    public static " + mappedReturn + " " + function + "(" + typedParamList + ") {\n"
                + body
                + "    }\n"
                + "}\n"
                + witnessClass;
        return new Realization(
                source,
                isStub,
                observedLossRecordJson(sugarEmissions, bodyLossRecord),
                usedSugarsJson(sugarEmissions),
                observationWrapperEmissionRecord);
    }

    private static String parameterAnnotations(List<SugarEmission> emissions, String paramName) {
        StringBuilder out = new StringBuilder();
        for (SugarEmission emission : emissions) {
            if (!emission.surfaceLocator().startsWith("annotation:")) continue;
            if (!paramName.equals(emission.symbol())) continue;
            if (!out.isEmpty()) out.append(" ");
            out.append(emission.rendered());
        }
        return out.toString();
    }

    private static List<String> modeList(String mode) {
        return mode == null || mode.isBlank() ? List.of() : List.of(mode);
    }

    private static List<String> modeList(String mode, List<String> modes) {
        List<String> out = new ArrayList<>();
        if (modes != null) {
            for (String m : modes) {
                if (m != null && !m.isBlank() && !out.contains(m)) out.add(m);
            }
        }
        if (out.isEmpty() && mode != null && !mode.isBlank()) {
            out.add(mode);
        }
        return out;
    }

    private static String observedLossRecordJson(List<SugarEmission> emissions) {
        return observedLossRecordJson(emissions, Jcs.object());
    }

    private static String observedLossRecordJson(List<SugarEmission> emissions, Jcs.Json bodyTemplateLossRecord) {
        Map<String, Jcs.Json> byDimension = new TreeMap<>();
        mergeLossRecord(byDimension, bodyTemplateLossRecord);
        for (SugarEmission emission : emissions) {
            mergeLossRecord(byDimension, emission.lossRecord());
        }
        List<Jcs.Field> fields = new ArrayList<>();
        for (Map.Entry<String, Jcs.Json> entry : byDimension.entrySet()) {
            fields.add(new Jcs.Field(entry.getKey(), entry.getValue()));
        }
        return Jcs.encode(new Jcs.Obj(fields));
    }

    private static void mergeLossRecord(Map<String, Jcs.Json> byDimension, Jcs.Json lossRecord) {
        if (lossRecord instanceof Jcs.Obj lossObj) {
            for (Jcs.Field field : lossObj.fields()) {
                byDimension.merge(field.key(), field.value(), SugarRealizer::combineLossFormula);
            }
        }
    }

    private static Jcs.Json combineLossRecords(Jcs.Json left, Jcs.Json right) {
        Map<String, Jcs.Json> byDimension = new TreeMap<>();
        mergeLossRecord(byDimension, left);
        mergeLossRecord(byDimension, right);
        List<Jcs.Field> fields = new ArrayList<>();
        for (Map.Entry<String, Jcs.Json> entry : byDimension.entrySet()) {
            fields.add(new Jcs.Field(entry.getKey(), entry.getValue()));
        }
        return new Jcs.Obj(fields);
    }

    private static Jcs.Json combineLossFormula(Jcs.Json existing, Jcs.Json next) {
        if (Jcs.encode(existing).equals(Jcs.encode(next))) {
            return existing;
        }
        return Jcs.object(
                "kind", Jcs.string("and"),
                "operands", Jcs.array(existing, next)
        );
    }

    private static Optional<ObservationComposition> composeEmitterAfterReturn(
            List<String> requestedModes,
            String operationBody,
            String mappedReturn,
            String function,
            String conceptName,
            ContractPayload contract) {
        if (!isEmitterOnly(requestedModes)
                || contract == null
                || !validCid(contract.conceptSiteCid())
                || !validCid(contract.objectFcmCid())
                || !validCid(contract.localContractCid())
                || conceptMatches("concept:contract-observation", conceptName)) {
            return Optional.empty();
        }
        Optional<RenderedBody> observation = renderBodyTemplateFor(
                "concept:contract-observation",
                List.of(contract.conceptSiteCid(), contract.localContractCid(), "emitter"),
                "emitter");
        if (observation.isEmpty()) {
            return Optional.empty();
        }
        String policyCid = emitterTagPolicyCid();
        String observationBody = observationTagBlock(contract, policyCid) + "\n" + observation.get().body();
        Optional<String> composed = composeAfterReturn(operationBody, observationBody, mappedReturn);
        if (composed.isEmpty()) {
            return Optional.empty();
        }
        String record = observationWrapperEmissionRecord(function, conceptName, mappedReturn, composed.get(), contract, policyCid);
        return Optional.of(new ObservationComposition(composed.get(), observation.get().lossRecord(), record));
    }

    private static boolean isEmitterOnly(List<String> requestedModes) {
        return requestedModes != null
                && requestedModes.size() == 1
                && "emitter".equals(requestedModes.get(0));
    }

    private static Optional<String> composeAfterReturn(String operationBody, String observationBody, String mappedReturn) {
        String trimmed = operationBody.stripTrailing();
        String[] lines = trimmed.split("\\R", -1);
        if (lines.length == 0) return Optional.empty();
        String last = lines[lines.length - 1].trim();
        if ("void".equals(mappedReturn)) {
            return Optional.of(trimmed + "\n" + observationBody);
        }
        if (!last.startsWith("return ") || !last.endsWith(";")) {
            return Optional.empty();
        }
        String expression = last.substring("return ".length(), last.length() - 1).trim();
        if (expression.isEmpty()) return Optional.empty();
        List<String> out = new ArrayList<>();
        for (int i = 0; i < lines.length - 1; i++) {
            out.add(lines[i]);
        }
        out.add(mappedReturn + " __provekit_result = " + expression + ";");
        out.add(observationBody);
        out.add("return __provekit_result;");
        return Optional.of(String.join("\n", out));
    }

    private static String observationWrapperEmissionRecord(
            String function,
            String conceptName,
            String mappedReturn,
            String composedBody,
            ContractPayload contract,
            String policyCid) {
        Jcs.Obj effect = logIoEffect(function, contract.localContractCid());
        Jcs.Obj wrapperFcm = Jcs.object(
                "autoMintedMementos", Jcs.array(),
                "bodyCid", Jcs.string(Blake3.blake3_512(composedBody.getBytes(StandardCharsets.UTF_8))),
                "effects", Jcs.array(effect),
                "fnName", Jcs.string(function + "$provekit_emitter"),
                "formalSorts", Jcs.array(),
                "formals", Jcs.array(),
                "kind", Jcs.string("function-contract"),
                "locus", Jcs.object("function", Jcs.string(function), "surface", Jcs.string("java-emitter-wrapper")),
                "post", Jcs.object("args", Jcs.array(), "kind", Jcs.string("atomic"), "name", Jcs.string("true")),
                "pre", Jcs.object("args", Jcs.array(), "kind", Jcs.string("atomic"), "name", Jcs.string("true")),
                "returnSort", Jcs.object("args", Jcs.array(), "kind", Jcs.string("ctor"), "name", Jcs.string(mappedReturn)),
                "schemaVersion", Jcs.string("1")
        );
        String wrapperFcmCid = Jcs.cid(wrapperFcm);
        Jcs.Obj preservationClaim = Jcs.object(
                "concept_name", Jcs.string(conceptName),
                "kind", Jcs.string("observation-preservation-claim"),
                "local_contract_cid", Jcs.string(contract.localContractCid()),
                "mode", Jcs.string("emitter"),
                "object_fcm_cid", Jcs.string(contract.objectFcmCid()),
                "policy_cid", Jcs.string(policyCid),
                "wrapper_fcm_cid", Jcs.string(wrapperFcmCid)
        );
        String preservationClaimCid = Jcs.cid(preservationClaim);
        return Jcs.encode(Jcs.object(
                "object_fcm_cid", Jcs.string(contract.objectFcmCid()),
                "observer_effects", Jcs.array(effect),
                "policy_cid", Jcs.string(policyCid),
                "preservation_claim_cid", Jcs.string(preservationClaimCid),
                "wrapper_fcm", wrapperFcm,
                "wrapper_fcm_cid", Jcs.string(wrapperFcmCid)
        ));
    }

    private static Jcs.Obj logIoEffect(String function, String contractCid) {
        return Jcs.object(
                "args", Jcs.object(
                        "channel", Jcs.string("java.util.logging"),
                        "contract_cid", Jcs.string(contractCid),
                        "operation", Jcs.string("log")
                ),
                "discharge_key", Jcs.string("io:java-util-logging:log"),
                "locator", Jcs.object("function", Jcs.string(function), "mode", Jcs.string("emitter")),
                "occurrence_kind", Jcs.string("Io"),
                "role", Jcs.string("body"),
                "signature_cid", Jcs.string(Blake3.blake3_512("java.util.logging.Logger.log(Level,String)".getBytes(StandardCharsets.UTF_8)))
        );
    }

    private static String observationTagBlock(ContractPayload contract, String policyCid) {
        return String.join("\n",
                "// provekit-observation: concept:contract-observation",
                "// provekit-observation-term: concept:contract-observation("
                        + contract.conceptSiteCid() + "," + contract.localContractCid() + ",emitter)",
                "// provekit-observation-mode: emitter",
                "// provekit-concept-site-cid: " + contract.conceptSiteCid(),
                "// provekit-object-fcm-cid: " + contract.objectFcmCid(),
                "// provekit-contract-cid: " + contract.localContractCid(),
                "// provekit-emitted-concept: concept:log-emit",
                "// provekit-observation-policy-cid: " + policyCid
        );
    }

    private static String emitterTagPolicyCid() {
        return Jcs.cid(Jcs.object(
                "emit_tags", Jcs.bool(true),
                "kind", Jcs.string("realization-emission-policy"),
                "modes", Jcs.array(Jcs.string("emitter")),
                "schemaVersion", Jcs.string("1"),
                "surface", Jcs.string("java-comment-tags")
        ));
    }

    private static boolean validCid(String cid) {
        if (cid == null || !cid.startsWith("blake3-512:") || cid.length() != "blake3-512:".length() + 128) {
            return false;
        }
        for (int i = "blake3-512:".length(); i < cid.length(); i++) {
            char ch = cid.charAt(i);
            if (!((ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f'))) {
                return false;
            }
        }
        return true;
    }

    private static String usedSugarsJson(List<SugarEmission> emissions) {
        List<Jcs.Json> used = new ArrayList<>();
        for (SugarEmission emission : emissions) {
            used.add(Jcs.object(
                    "cid", Jcs.string(emission.sugarCid()),
                    "loss_record", emission.lossRecord(),
                    "sugar_name", Jcs.string(emission.sugarName()),
                    "surface_locator", Jcs.string(emission.surfaceLocator())
            ));
        }
        return Jcs.encode(Jcs.array(used));
    }

    private static String importsFor(
            List<SugarEmission> emissions,
            boolean hasMethodNotNull,
            boolean hasJUnitWitness) {
        StringBuilder imports = new StringBuilder();
        if (hasMethodNotNull || hasRenderedAnnotation(emissions, "@NotNull")) {
            imports.append("import jakarta.validation.constraints.NotNull;\n");
        }
        if (hasRenderedAnnotation(emissions, "@Min")) {
            imports.append("import jakarta.validation.constraints.Min;\n");
        }
        if (hasRenderedAnnotation(emissions, "@Max")) {
            imports.append("import jakarta.validation.constraints.Max;\n");
        }
        if (hasRenderedAnnotation(emissions, "@Size")) {
            imports.append("import jakarta.validation.constraints.Size;\n");
        }
        if (hasJUnitWitness) {
            imports.append("import org.junit.jupiter.api.Disabled;\n");
            imports.append("import org.junit.jupiter.api.Test;\n");
            imports.append("import static org.junit.jupiter.api.Assertions.assertNotNull;\n");
        }
        if (!imports.isEmpty()) {
            imports.append("\n");
        }
        return imports.toString();
    }

    private static boolean hasRenderedAnnotation(List<SugarEmission> emissions, String annotationName) {
        return emissions.stream().anyMatch(e ->
                e.surfaceLocator().startsWith("annotation:") && e.rendered().startsWith(annotationName));
    }

    private static String junitWitnessBody(List<SugarEmission> emissions) {
        StringBuilder out = new StringBuilder();
        Set<String> declared = new TreeSet<>();
        for (SugarEmission emission : emissions) {
            if (emission.surfaceLocator().startsWith("witness:junit5") && !emission.symbol().isBlank()) {
                String symbol = sanitizeJavaIdentifier(emission.symbol());
                if (declared.add(symbol)) {
                    out.append("        Object ").append(symbol).append(" = null;\n");
                }
            }
        }
        for (SugarEmission emission : emissions) {
            if (emission.surfaceLocator().startsWith("witness:junit5")) {
                out.append("        ").append(emission.rendered()).append("\n");
            }
        }
        return out.toString();
    }

    private static String sanitizeJavaIdentifier(String raw) {
        if (raw == null || raw.isBlank()) return "value";
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < raw.length(); i++) {
            char ch = raw.charAt(i);
            boolean ok = i == 0 ? Character.isJavaIdentifierStart(ch) : Character.isJavaIdentifierPart(ch);
            out.append(ok ? ch : '_');
        }
        if (out.isEmpty() || !Character.isJavaIdentifierStart(out.charAt(0))) {
            return "value_" + out;
        }
        return out.toString();
    }

    private static String contractPrefix(ContractPayload contract) {
        if (contract == null || contract.localContractCid().isEmpty()) {
            return "";
        }
        StringBuilder out = new StringBuilder();
        out.append("    // contract-cid: ").append(contract.localContractCid()).append("\n");
        out.append("    // contract-mode: ").append(contract.dischargeVerdict()).append("\n");
        Set<String> kinds = new TreeSet<>();
        for (ContractWitness witness : contract.witnesses()) {
            if (!witness.sourceKind().isEmpty()) {
                kinds.add(witness.sourceKind());
            }
        }
        for (String kind : kinds) {
            out.append("    // contract-source: ").append(kind).append("\n");
        }
        return out.toString();
    }

    private static String commentPrefix(ContractPayload contract, List<SugarEmission> emissions) {
        StringBuilder out = new StringBuilder();
        for (SugarEmission emission : emissions) {
            if (emission.surfaceLocator().startsWith("comment:")) {
                if (contract != null) {
                    out.append(contractCommentTagBlock(contract, emission));
                }
                out.append("    ").append(emission.rendered()).append("\n");
            }
        }
        return out.toString();
    }

    private static String contractCommentTagBlock(ContractPayload contract, SugarEmission emission) {
        Jcs.Obj payload = contractCommentPayload(contract, emission);
        StringBuilder out = new StringBuilder();
        out.append("    // provekit-contract: ").append(Jcs.encode(payload)).append("\n");
        out.append("    // provekit-contract-payload-cid: ").append(Jcs.cid(payload)).append("\n");
        return out.toString();
    }

    private static Jcs.Obj contractCommentPayload(ContractPayload contract, SugarEmission emission) {
        return Jcs.object(
                "artifact_kind", Jcs.string("provekit-contract-comment-sugar"),
                "concept_site_cid", Jcs.string(contract.conceptSiteCid()),
                "contract_cid", Jcs.string(contract.localContractCid()),
                "emitted_by", Jcs.object(
                        "kit_cid", Jcs.string(contractCommentEmitterKitCid()),
                        "kit_kind", Jcs.string("realize"),
                        "target_language", Jcs.string("java")
                ),
                "fol_text", Jcs.string(commentLineValue(emission.predicateText())),
                "ir_formula_jcs", emission.predicate(),
                "ir_formula_jcs_cid", Jcs.string(Jcs.cid(emission.predicate())),
                "local_contract_cid", Jcs.string(contract.localContractCid()),
                "loss_record_cid", Jcs.string(Jcs.cid(emission.lossRecord())),
                "policy_cid", Jcs.string(contractCommentPolicyCid()),
                "role", Jcs.string(emission.role()),
                "schema_version", Jcs.string("1"),
                "sugar_dict_cid", Jcs.string(emission.sugarCid())
        );
    }

    private static String contractCommentPolicyCid() {
        return Jcs.cid(Jcs.object(
                "emit_contract_tags", Jcs.bool(true),
                "kind", Jcs.string("realization-emission-policy"),
                "schemaVersion", Jcs.string("1"),
                "surface", Jcs.string("java-contract-comment")
        ));
    }

    private static String contractCommentEmitterKitCid() {
        return Jcs.cid(Jcs.object(
                "kit_kind", Jcs.string("realize"),
                "name", Jcs.string("provekit-realize-java"),
                "target_language", Jcs.string("java")
        ));
    }

    private static String commentLineValue(String raw) {
        return raw == null ? "" : raw.replace('\n', ' ').replace('\r', ' ').trim();
    }

    private static boolean contractHasNonNullPrecondition(ContractPayload contract, String param) {
        if (contract == null) return false;
        for (ContractWitness witness : contract.witnesses()) {
            if ("pre".equals(witness.role())
                    && (isNonNullPredicateFor(witness.predicateText(), param)
                    || isNeqNullPredicateFor(witness.predicate(), param))) {
                return true;
            }
        }
        return false;
    }

    private static boolean contractHasNonNullPostcondition(ContractPayload contract) {
        if (contract == null) return false;
        for (ContractWitness witness : contract.witnesses()) {
            if ("post".equals(witness.role())
                    && (isNonNullPredicateFor(witness.predicateText(), "out")
                    || isNonNullPredicateFor(witness.predicateText(), "return")
                    || isNonNullPredicateFor(witness.predicateText(), "result")
                    || isNeqNullPredicateFor(witness.predicate(), "out")
                    || isNeqNullPredicateFor(witness.predicate(), "return")
                    || isNeqNullPredicateFor(witness.predicate(), "result"))) {
                return true;
            }
        }
        return false;
    }

    private static boolean isNonNullPredicateFor(String predicateText, String symbol) {
        String normalized = predicateText == null ? "" : predicateText.replaceAll("\\s+", "");
        return normalized.equals("non_null(" + symbol + ")")
                || normalized.equals(symbol + "!=null")
                || normalized.equals("not_null(" + symbol + ")");
    }

    private static boolean isNeqNullPredicateFor(Jcs.Json predicate, String symbol) {
        if (!(predicate instanceof Jcs.Obj obj)) return false;
        if (!"atomic".equals(obj.stringFieldOrNull("kind"))) return false;
        if (!"neq".equals(obj.stringFieldOrNull("name"))) return false;
        Jcs.Json argsJson = obj.get("args");
        if (!(argsJson instanceof Jcs.Arr args) || args.values().size() != 2) return false;
        return isVarNamed(args.get(0), symbol) && isConstNullTerm(args.get(1));
    }

    private static boolean isVarNamed(Jcs.Json term, String symbol) {
        return term instanceof Jcs.Obj obj
                && "var".equals(obj.stringFieldOrNull("kind"))
                && symbol.equals(obj.stringFieldOrNull("name"));
    }

    private static boolean isConstNullTerm(Jcs.Json term) {
        return term instanceof Jcs.Obj obj
                && "const".equals(obj.stringFieldOrNull("kind"))
                && obj.get("value") instanceof Jcs.Null;
    }

    /**
     * Map a Rust source type to the Java equivalent.
     *
     * Mirrors cmd_transport.rs map_source_type for TargetStyle::Java.
     */
    static String mapSourceType(String src) {
        return switch (src) {
            case "()" -> "void";
            case "i64", "u64" -> "long";
            case "i32", "u32" -> "int";
            case "i16", "u16" -> "short";
            case "i8", "u8" -> "byte";
            case "f64" -> "double";
            case "f32" -> "float";
            case "bool" -> "boolean";
            case "String", "&str", "&String" -> "String";
            default -> src;
        };
    }

    /**
     * Convert snake_case to PascalCase.
     *
     * Mirrors cmd_transport.rs snake_to_pascal_local.
     */
    static String snakeToPascal(String s) {
        StringBuilder sb = new StringBuilder();
        for (String part : s.split("_", -1)) {
            if (part.isEmpty()) continue;
            sb.append(Character.toUpperCase(part.charAt(0)));
            if (part.length() > 1) sb.append(part.substring(1));
        }
        return sb.toString();
    }

    // -----------------------------------------------------------------------
    // Body-template loading per protocol/specs/2026-05-13-body-template-memento.md
    // -----------------------------------------------------------------------

    /**
     * Cached body-template entries, loaded lazily on first call from the
     * classpath resource `com/provekit/realize/java-canonical-bodies.json`.
     * The resource is sourced from
     * `menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json`
     * at build time (see pom.xml resource configuration).
     */
    private static volatile List<BodyTemplateEntry> ENTRIES_CACHE = null;

    /**
     * One body-template entry per spec §2.
     */
    private record BodyTemplateEntry(
            String conceptName,
            String mode,
            String templateKind,
            String template,
            List<TemplateCitation> citations,
            Jcs.Json lossRecord,
            Integer minParams,
            Integer maxParams) {}

    private record TemplateCitation(
            String placeholder,
            String conceptName,
            String mode,
            List<String> params) {}

    private record RenderedBody(String body, Jcs.Json lossRecord) {}

    private record ObservationComposition(
            String body,
            Jcs.Json lossRecord,
            String observationWrapperEmissionRecord) {}

    /**
     * Render a body string for the given concept + signature, or empty when
     * no entry matches (caller falls back to the language stub).
     *
     * Selection per spec §2.2: exact concept_name match + signature_guard
     * pass (min_params <= |params| <= max_params when present). Substitution
     * per §2.3: ${param0}, ${param1}, ... ${param_count}. Unbound placeholders
     * refuse-match.
     */
    static Optional<String> bodyTemplateFor(String conceptName, List<String> params) {
        return bodyTemplateFor(conceptName, params, "");
    }

    static Optional<String> bodyTemplateFor(String conceptName, List<String> params, String mode) {
        return renderBodyTemplateFor(conceptName, params, mode).map(RenderedBody::body);
    }

    private static Optional<RenderedBody> renderBodyTemplateFor(String conceptName, List<String> params, String mode) {
        return renderBodyTemplateFor(conceptName, params, mode, new ArrayList<>());
    }

    private static Optional<RenderedBody> renderBodyTemplateFor(
            String conceptName,
            List<String> params,
            String mode,
            List<String> recursionStack) {
        String stackKey = conceptName + "#" + (mode == null ? "" : mode);
        if (recursionStack.contains(stackKey) || recursionStack.size() >= 8) {
            return Optional.empty();
        }
        recursionStack.add(stackKey);
        try {
            return bodyTemplateForUntracked(conceptName, params, mode, recursionStack);
        } finally {
            recursionStack.remove(recursionStack.size() - 1);
        }
    }

    private static Optional<RenderedBody> bodyTemplateForUntracked(
            String conceptName,
            List<String> params,
            String mode,
            List<String> recursionStack) {
        List<BodyTemplateEntry> entries = entries();
        for (BodyTemplateEntry e : entries) {
            if (!conceptMatches(e.conceptName(), conceptName)) continue;
            if (!modeMatches(e.mode(), mode)) continue;
            if (e.minParams() != null && params.size() < e.minParams()) continue;
            if (e.maxParams() != null && params.size() > e.maxParams()) continue;
            if (!"verbatim".equals(e.templateKind())) continue;
            Optional<RenderedBody> rendered = renderTemplate(e, params, recursionStack);
            if (rendered.isPresent()) {
                return rendered;
            }
        }
        return Optional.empty();
    }

    private static Optional<RenderedBody> renderTemplate(
            BodyTemplateEntry entry,
            List<String> params,
            List<String> recursionStack) {
        Optional<String> renderedMaybe = substituteTemplateBindings(entry.template(), params);
        if (renderedMaybe.isEmpty()) {
            return Optional.empty();
        }
        String rendered = renderedMaybe.get();
        Jcs.Json lossRecord = entry.lossRecord();
        for (TemplateCitation citation : entry.citations()) {
            List<String> citationParams = new ArrayList<>();
            for (String rawParam : citation.params()) {
                Optional<String> renderedParam = substituteTemplateBindings(rawParam, params);
                if (renderedParam.isEmpty()) {
                    return Optional.empty();
                }
                String renderedParamText = renderedParam.get();
                if (renderedParamText.contains("${")) {
                    return Optional.empty();
                }
                citationParams.add(renderedParamText);
            }
            Optional<RenderedBody> citationBody = renderBodyTemplateFor(
                    citation.conceptName(),
                    citationParams,
                    citation.mode(),
                    recursionStack);
            if (citationBody.isEmpty()) {
                return Optional.empty();
            }
            rendered = rendered.replace("${" + citation.placeholder() + "}", citationBody.get().body());
            lossRecord = combineLossRecords(lossRecord, citationBody.get().lossRecord());
        }
        if (rendered.contains("${")) {
            // Unbound placeholder: refuse-match per spec §2.1.
            return Optional.empty();
        }
        return Optional.of(new RenderedBody(rendered, lossRecord));
    }

    private static Optional<String> substituteTemplateBindings(String template, List<String> params) {
        String rendered = template;
        for (int i = 0; i < params.size(); i++) {
            String julLevel = "${param" + i + "_jul_level}";
            if (rendered.contains(julLevel)) {
                Optional<String> mappedLevel = javaUtilLoggingLevel(params.get(i));
                if (mappedLevel.isEmpty()) {
                    return Optional.empty();
                }
                rendered = rendered.replace(julLevel, mappedLevel.get());
            }
            rendered = rendered.replace("${param" + i + "}", params.get(i));
        }
        return Optional.of(rendered.replace("${param_count}", Integer.toString(params.size())));
    }

    private static Optional<String> javaUtilLoggingLevel(String rawLevel) {
        if (rawLevel == null) return Optional.empty();
        return switch (rawLevel.trim().toLowerCase(Locale.ROOT)) {
            case "trace" -> Optional.of("FINEST");
            case "debug" -> Optional.of("FINE");
            case "info" -> Optional.of("INFO");
            case "warn" -> Optional.of("WARNING");
            case "error", "fatal" -> Optional.of("SEVERE");
            default -> Optional.empty();
        };
    }

    private static boolean conceptMatches(String entryName, String requestName) {
        if (entryName.equals(requestName)) return true;
        if (entryName.startsWith("concept:") && entryName.substring("concept:".length()).equals(requestName)) {
            return true;
        }
        return requestName.startsWith("concept:")
                && requestName.substring("concept:".length()).equals(entryName);
    }

    private static boolean modeMatches(String entryMode, String requestMode) {
        if (entryMode == null || entryMode.isBlank()) return true;
        return requestMode != null && !requestMode.isBlank() && entryMode.equals(requestMode);
    }

    private static List<BodyTemplateEntry> entries() {
        List<BodyTemplateEntry> cached = ENTRIES_CACHE;
        if (cached != null) return cached;
        synchronized (SugarRealizer.class) {
            if (ENTRIES_CACHE != null) return ENTRIES_CACHE;
            ENTRIES_CACHE = loadEntriesFromResource();
            return ENTRIES_CACHE;
        }
    }

    private static List<BodyTemplateEntry> loadEntriesFromResource() {
        try (InputStream in = SugarRealizer.class.getResourceAsStream("java-canonical-bodies.json")) {
            if (in == null) {
                // Resource absent: degrade to "no entries" (callers get language stub).
                return List.of();
            }
            String raw;
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8))) {
                raw = reader.lines().collect(Collectors.joining("\n"));
            }
            Jcs.Json root = Jcs.parse(raw);
            if (!(root instanceof Jcs.Obj rootObj)) return List.of();
            Jcs.Json header = rootObj.get("header");
            if (!(header instanceof Jcs.Obj headerObj)) return List.of();
            Jcs.Json content = headerObj.get("content");
            if (!(content instanceof Jcs.Obj contentObj)) return List.of();
            Jcs.Json entriesJson = contentObj.get("entries");
            if (!(entriesJson instanceof Jcs.Arr entriesArr)) return List.of();

            List<BodyTemplateEntry> out = new ArrayList<>();
            for (Jcs.Json item : entriesArr.values()) {
                if (!(item instanceof Jcs.Obj itemObj)) continue;
                String conceptName = itemObj.stringFieldOrNull("concept_name");
                if (conceptName == null) continue;
                String mode = itemObj.stringFieldOrNull("mode");

                Jcs.Json template = itemObj.get("emission_template");
                if (!(template instanceof Jcs.Obj templateObj)) continue;
                String kind = templateObj.stringFieldOrNull("kind");
                String tmpl = templateObj.stringFieldOrNull("template");
                if (kind == null || tmpl == null) continue;
                Optional<List<TemplateCitation>> citations = templateCitations(templateObj);
                if (citations.isEmpty()) continue;

                Integer minParams = null;
                Integer maxParams = null;
                Jcs.Json guard = itemObj.get("signature_guard");
                if (guard instanceof Jcs.Obj guardObj) {
                    Jcs.Json minJ = guardObj.get("min_params");
                    Jcs.Json maxJ = guardObj.get("max_params");
                    if (minJ instanceof Jcs.Num minN) minParams = (int) minN.value();
                    if (maxJ instanceof Jcs.Num maxN) maxParams = (int) maxN.value();
                }
                out.add(new BodyTemplateEntry(
                        conceptName,
                        mode,
                        kind,
                        tmpl,
                        citations.get(),
                        lossRecordValue(itemObj),
                        minParams,
                        maxParams));
            }
            return out;
        } catch (IOException e) {
            // I/O failure: degrade to "no entries"; stubs will emit.
            return List.of();
        }
    }

    private static Optional<List<TemplateCitation>> templateCitations(Jcs.Obj templateObj) {
        Jcs.Json citationsJson = templateObj.get("citations");
        if (citationsJson == null) return Optional.of(List.of());
        if (!(citationsJson instanceof Jcs.Arr citationsArr)) return Optional.empty();
        List<TemplateCitation> citations = new ArrayList<>();
        for (Jcs.Json raw : citationsArr.values()) {
            if (!(raw instanceof Jcs.Obj citationObj)) return Optional.empty();
            String placeholder = citationObj.stringFieldOrNull("placeholder");
            String conceptName = citationObj.stringFieldOrNull("concept_name");
            if (placeholder == null || placeholder.isBlank() || conceptName == null || conceptName.isBlank()) {
                return Optional.empty();
            }
            String mode = citationObj.stringFieldOrNull("mode");
            List<String> params = new ArrayList<>();
            Jcs.Json paramsJson = citationObj.get("params");
            if (!(paramsJson instanceof Jcs.Arr paramsArr)) return Optional.empty();
            for (Jcs.Json param : paramsArr.values()) {
                if (param instanceof Jcs.Str stringParam) {
                    params.add(stringParam.value());
                } else {
                    return Optional.empty();
                }
            }
            citations.add(new TemplateCitation(placeholder, conceptName, mode, params));
        }
        return Optional.of(List.copyOf(citations));
    }

    private static Jcs.Json lossRecordValue(Jcs.Obj entryObj) {
        Jcs.Json contribution = entryObj.get("loss_record_contribution");
        if (!(contribution instanceof Jcs.Obj contributionObj)) return Jcs.object();
        if (!"literal".equals(contributionObj.stringFieldOrNull("form"))) return Jcs.object();
        Jcs.Json value = contributionObj.get("value");
        return value instanceof Jcs.Obj ? value : Jcs.object();
    }
}

record SugarEmission(
        String sugarCid,
        String sugarName,
        String surfaceLocator,
        String rendered,
        String symbol,
        String role,
        Jcs.Json predicate,
        String predicateText,
        Jcs.Json lossRecord) {}

final class SugarDictionary {
    private SugarDictionary() {}

    static List<SugarEmission> emitAll(ContractPayload contract, List<String> pluginJson, List<String> requestModes) {
        if (contract == null || pluginJson == null || pluginJson.isEmpty()) {
            return List.of();
        }
        List<SugarEmission> out = new ArrayList<>();
        for (String rawPlugin : pluginJson) {
            SugarPlugin plugin = SugarPlugin.fromJson(rawPlugin);
            if (plugin == null || !"java".equals(plugin.targetLanguage())) continue;
            for (ContractWitness witness : contract.witnesses()) {
                Jcs.Json predicate = witness.predicate();
                for (SugarEntry entry : plugin.entries()) {
                    if (!modeMatches(entry.mode(), requestModes)) continue;
                    Match match = match(predicate, entry.pattern(), entry.template(), witness);
                    if (match != null) {
                        out.add(new SugarEmission(
                                plugin.cid(),
                                plugin.sugarName(),
                                entry.surfaceLocator(),
                                render(entry.template(), match),
                                match.symbol(),
                                witness.role(),
                                witness.predicate(),
                                witness.predicateText(),
                                entry.lossRecord()
                        ));
                    }
                }
            }
        }
        return out;
    }

    private static boolean modeMatches(String entryMode, List<String> requestModes) {
        if (entryMode == null || entryMode.isBlank()) return true;
        return requestModes != null && requestModes.contains(entryMode);
    }

    private static Match match(Jcs.Json predicate, Jcs.Json pattern, String template, ContractWitness witness) {
        if (!(pattern instanceof Jcs.Obj patternObj)) return null;
        String patternName = patternObj.stringFieldOrNull("name");
        if (patternName != null && patternName.startsWith("${")) {
            return new Match(witnessSymbol(witness), witness.predicateText(), witness.role(), Map.of());
        }
        if (!(predicate instanceof Jcs.Obj predicateObj)) return null;
        if (!"atomic".equals(predicateObj.stringFieldOrNull("kind"))) return null;
        if (patternName == null || !patternName.equals(predicateObj.stringFieldOrNull("name"))) return null;
        Jcs.Json patternArgsJson = patternObj.get("args");
        Jcs.Json predicateArgsJson = predicateObj.get("args");
        if (!(patternArgsJson instanceof Jcs.Arr patternArgs)
                || !(predicateArgsJson instanceof Jcs.Arr predicateArgs)
                || patternArgs.values().size() != predicateArgs.values().size()) {
            return null;
        }
        Map<String, String> bindings = new TreeMap<>();
        for (int i = 0; i < patternArgs.values().size(); i++) {
            Jcs.Json p = patternArgs.get(i);
            Jcs.Json actual = predicateArgs.get(i);
            if (bindTerm(p, actual, template, bindings)) {
                continue;
            }
            if (!Jcs.encode(p).equals(Jcs.encode(actual))) {
                return null;
            }
        }
        return new Match(bindings.getOrDefault("symbol", ""), witness.predicateText(), witness.role(), bindings);
    }

    private static boolean bindTerm(Jcs.Json pattern, Jcs.Json actual, String template, Map<String, String> bindings) {
        if (pattern instanceof Jcs.Obj patternObj
                && "var".equals(patternObj.stringFieldOrNull("kind"))
                && actual instanceof Jcs.Obj actualObj
                && "var".equals(actualObj.stringFieldOrNull("kind"))) {
            String hole = holeName(patternObj.stringFieldOrNull("name"));
            String actualName = actualObj.stringFieldOrNull("name");
            if (hole != null && actualName != null) {
                bindings.put(hole, actualName);
                return true;
            }
        }
        if (pattern instanceof Jcs.Obj patternObj
                && "const".equals(patternObj.stringFieldOrNull("kind"))
                && patternObj.get("value") instanceof Jcs.Str holeValue
                && actual instanceof Jcs.Obj actualObj
                && "const".equals(actualObj.stringFieldOrNull("kind"))
                && actualObj.get("value") instanceof Jcs.Num number) {
            String hole = holeName(holeValue.value());
            if (hole != null) {
                long value = number.value();
                String plusOne = null;
                String minusOne = null;
                try {
                    if (referencesTemplateBinding(template, hole + "_plus_one")) {
                        plusOne = Long.toString(Math.addExact(value, 1L));
                    }
                    if (referencesTemplateBinding(template, hole + "_minus_one")) {
                        minusOne = Long.toString(Math.subtractExact(value, 1L));
                    }
                } catch (ArithmeticException overflow) {
                    return false;
                }
                bindings.put(hole, Long.toString(value));
                if (plusOne != null) {
                    bindings.put(hole + "_plus_one", plusOne);
                }
                if (minusOne != null) {
                    bindings.put(hole + "_minus_one", minusOne);
                }
                return true;
            }
        }
        return isConstNullPattern(pattern) && isConstNullPattern(actual);
    }

    private static boolean referencesTemplateBinding(String template, String binding) {
        return template != null && template.contains("${" + binding + "}");
    }

    private static String holeName(String raw) {
        if (raw == null || !raw.startsWith("${") || !raw.endsWith("}")) return null;
        return raw.substring(2, raw.length() - 1);
    }

    private static boolean isConstNullPattern(Jcs.Json json) {
        return json instanceof Jcs.Obj obj
                && "const".equals(obj.stringFieldOrNull("kind"))
                && obj.get("value") instanceof Jcs.Null;
    }

    private static String witnessSymbol(ContractWitness witness) {
        Jcs.Json predicate = witness.predicate();
        if (predicate instanceof Jcs.Obj obj && obj.get("args") instanceof Jcs.Arr args && !args.isEmpty()) {
            Jcs.Json first = args.get(0);
            if (first instanceof Jcs.Obj termObj && "var".equals(termObj.stringFieldOrNull("kind"))) {
                return termObj.stringFieldOrNull("name");
            }
        }
        return "value";
    }

    private static String render(String template, Match match) {
        String rendered = template
                .replace("${symbol}", match.symbol())
                .replace("${formula_pretty_print}", match.formulaPrettyPrint())
                .replace("${contract_role}", roleKeyword(match.role()));
        for (Map.Entry<String, String> binding : match.bindings().entrySet()) {
            rendered = rendered.replace("${" + binding.getKey() + "}", binding.getValue());
        }
        return rendered;
    }

    private static String roleKeyword(String role) {
        return switch (role) {
            case "pre" -> "requires";
            case "post" -> "ensures";
            default -> "contract";
        };
    }

    private record Match(String symbol, String formulaPrettyPrint, String role, Map<String, String> bindings) {}

    private record SugarPlugin(String cid, String sugarName, String targetLanguage, List<SugarEntry> entries) {
        static SugarPlugin fromJson(String raw) {
            try {
                Jcs.Json rootJson = Jcs.parse(raw);
                if (!(rootJson instanceof Jcs.Obj root)) return null;
                Jcs.Json headerJson = root.get("header");
                if (!(headerJson instanceof Jcs.Obj header)) return null;
                String cid = header.stringFieldOrNull("cid");
                Jcs.Json contentJson = header.get("content");
                if (!(contentJson instanceof Jcs.Obj content)) return null;
                String sugarName = content.stringFieldOrNull("sugar_name");
                String targetLanguage = content.stringFieldOrNull("target_language");
                Jcs.Json entriesJson = content.get("entries");
                if (!(entriesJson instanceof Jcs.Arr entriesArr)) return null;
                List<SugarEntry> entries = new ArrayList<>();
                for (Jcs.Json entryJson : entriesArr.values()) {
                    if (entryJson instanceof Jcs.Obj entryObj) {
                        SugarEntry entry = SugarEntry.fromJson(entryObj);
                        if (entry != null) entries.add(entry);
                    }
                }
                return new SugarPlugin(cid == null ? "" : cid, sugarName, targetLanguage, entries);
            } catch (IllegalArgumentException e) {
                return null;
            }
        }
    }

    private record SugarEntry(String surfaceLocator, String template, String mode, Jcs.Json pattern, Jcs.Json lossRecord) {
        static SugarEntry fromJson(Jcs.Obj entry) {
            Jcs.Json templateJson = entry.get("emission_template");
            if (!(templateJson instanceof Jcs.Obj templateObj)) return null;
            Jcs.Json lossJson = entry.get("loss_record_contribution");
            if (!(lossJson instanceof Jcs.Obj lossObj)) return null;
            Jcs.Json pattern = entry.get("predicate_pattern");
            if (pattern == null) return null;
            Jcs.Json value = lossObj.get("value");
            return new SugarEntry(
                    templateObj.stringFieldOrNull("surface_locator"),
                    templateObj.stringFieldOrNull("template"),
                    entry.stringFieldOrNull("mode"),
                    pattern,
                    value == null ? new Jcs.Obj(List.of()) : value
            );
        }
    }
}

record ContractWitness(String role, Jcs.Json predicate, String predicateText, String sourceKind) {
    ContractWitness(String role, String predicateText, String sourceKind) {
        this(role, predicateFromText(predicateText), predicateText, sourceKind);
    }

    ContractWitness {
        role = role == null ? "" : role;
        predicate = predicate == null ? new Jcs.Null() : predicate;
        predicateText = predicateText == null ? "" : predicateText;
        sourceKind = sourceKind == null ? "" : sourceKind;
    }

    private static Jcs.Json parsePredicate(String json, String fallbackText) {
        if (json != null && !json.isBlank()) {
            try {
                return Jcs.parse(json);
            } catch (IllegalArgumentException ignored) {
            }
        }
        return predicateFromText(fallbackText);
    }

    private static Jcs.Json predicateFromText(String text) {
        String normalized = text == null ? "" : text.replaceAll("\\s+", "");
        String symbol = symbolInside(normalized, "non_null");
        if (symbol.isEmpty()) symbol = symbolInside(normalized, "not_null");
        if (symbol.isEmpty() && normalized.endsWith("!=null")) {
            symbol = normalized.substring(0, normalized.length() - "!=null".length());
        }
        if (!symbol.isEmpty()) {
            return Jcs.object(
                    "args", Jcs.array(
                            Jcs.object("kind", Jcs.string("var"), "name", Jcs.string(symbol)),
                            Jcs.object(
                                    "kind", Jcs.string("const"),
                                    "sort", Jcs.object("kind", Jcs.string("primitive"), "name", Jcs.string("Ref")),
                                    "value", Jcs.nullValue()
                            )
                    ),
                    "kind", Jcs.string("atomic"),
                    "name", Jcs.string("neq")
            );
        }
        return new Jcs.Null();
    }

    private static String symbolInside(String text, String fn) {
        String prefix = fn + "(";
        return text.startsWith(prefix) && text.endsWith(")")
                ? text.substring(prefix.length(), text.length() - 1)
                : "";
    }

    static ContractWitness fromJson(String json) {
        String predicateJson = JsonUtil.extractObjectField(json, "predicate");
        String predicateText = JsonUtil.decodeJsonStringField(json, "predicate_text");
        return new ContractWitness(
                JsonUtil.decodeJsonStringField(json, "role"),
                parsePredicate(predicateJson, predicateText),
                firstNonEmpty(predicateText, predicateJson),
                JsonUtil.decodeJsonStringField(json, "source_kind")
        );
    }

    private static String firstNonEmpty(String first, String second) {
        return first == null || first.isEmpty() ? second : first;
    }
}

record ContractPayload(
        String conceptSiteCid,
        String objectFcmCid,
        String localContractCid,
        String origin,
        String dischargeVerdict,
        List<ContractWitness> witnesses) {
    ContractPayload {
        conceptSiteCid = conceptSiteCid == null ? "" : conceptSiteCid;
        objectFcmCid = objectFcmCid == null ? "" : objectFcmCid;
        localContractCid = localContractCid == null ? "" : localContractCid;
        origin = origin == null ? "" : origin;
        dischargeVerdict = dischargeVerdict == null ? "" : dischargeVerdict;
        witnesses = witnesses == null ? List.of() : List.copyOf(witnesses);
    }

    ContractPayload(
            String conceptSiteCid,
            String localContractCid,
            String origin,
            String dischargeVerdict,
            List<ContractWitness> witnesses) {
        this(conceptSiteCid, localContractCid, localContractCid, origin, dischargeVerdict, witnesses);
    }

    static ContractPayload fromJson(String json) {
        if (json == null || json.isBlank() || "{}".equals(json.trim())) {
            return null;
        }
        List<ContractWitness> witnesses = JsonUtil.decodeJsonObjectArray(json, "witnesses")
                .stream()
                .map(ContractWitness::fromJson)
                .toList();
        return new ContractPayload(
                JsonUtil.decodeJsonStringField(json, "concept_site_cid"),
                JsonUtil.decodeJsonStringField(json, "object_fcm_cid"),
                JsonUtil.decodeJsonStringField(json, "local_contract_cid"),
                JsonUtil.decodeJsonStringField(json, "origin"),
                JsonUtil.decodeJsonStringField(json, "discharge_verdict"),
                witnesses
        );
    }
}
