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
            String observationWrapperEmissionRecord,
            /// #1390: static field helpers needed by the body. Empty for
            /// stubs and bodies that don't reference class-level statics.
            List<String> helpers) {
        Realization(String source, boolean isStub, String observedLossRecord,
                   String usedSugarsJson, String observationWrapperEmissionRecord) {
            this(source, isStub, observedLossRecord, usedSugarsJson,
                 observationWrapperEmissionRecord, List.of());
        }
    }

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
        return emitStub(function, params, paramTypes, returnType, conceptName, mode, modes, contract, sugarPluginJson, null);
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
            List<String> sugarPluginJson,
            TransportedOperation transportedOp) {
        return emitStub(function, params, paramTypes, returnType, conceptName, mode, modes, contract, sugarPluginJson, transportedOp, null, null);
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
            List<String> sugarPluginJson,
            TransportedOperation transportedOp,
            String termShapeJson,
            String operandBindingsJson) {
        // Legacy entry point (same-language realize, no cross-lang signaling).
        // Routes to the full overload with isCrossLang=false so the
        // substrate-gap refusal guard does NOT fire on empty sort CIDs —
        // those empties are absence-of-signal, not declared-gap.
        return emitStub(function, params, paramTypes, List.of(), returnType, "", conceptName,
                mode, modes, contract, sugarPluginJson, transportedOp, termShapeJson,
                operandBindingsJson, false);
    }

    /**
     * Substrate-honest signature emission: when `paramSortCids` /
     * `returnSortCid` are populated (cross-language materialize), the
     * realize binary resolves java syntax via the kit's concept-hub →
     * java map (mapConceptHubSortCidToJava). Falls back to mapSourceType
     * on the raw source-language strings when sort CIDs are absent.
     */
    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            List<String> paramSortCids,
            String returnType,
            String returnSortCid,
            String conceptName,
            String mode,
            List<String> modes,
            ContractPayload contract,
            List<String> sugarPluginJson,
            TransportedOperation transportedOp,
            String termShapeJson,
            String operandBindingsJson,
            boolean isCrossLang) {
        return emitStub(function, params, paramTypes, paramSortCids, returnType, returnSortCid,
                conceptName, mode, modes, contract, sugarPluginJson, transportedOp, termShapeJson,
                operandBindingsJson, isCrossLang, "");
    }

    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            List<String> paramSortCids,
            String returnType,
            String returnSortCid,
            String conceptName,
            String mode,
            List<String> modes,
            ContractPayload contract,
            List<String> sugarPluginJson,
            TransportedOperation transportedOp,
            String termShapeJson,
            String operandBindingsJson,
            boolean isCrossLang,
            String targetLibraryTag) {
        return emitStub(function, params, paramTypes, paramSortCids, returnType, returnSortCid,
                conceptName, mode, modes, contract, sugarPluginJson, transportedOp,
                termShapeJson, operandBindingsJson, isCrossLang, targetLibraryTag, List.of());
    }

    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            List<String> paramSortCids,
            String returnType,
            String returnSortCid,
            String conceptName,
            String mode,
            List<String> modes,
            ContractPayload contract,
            List<String> sugarPluginJson,
            TransportedOperation transportedOp,
            String termShapeJson,
            String operandBindingsJson,
            boolean isCrossLang,
            String targetLibraryTag,
            List<ParametricExpansion> parametricExpansions) {
        // Library-tag scope: set thread-local for the duration of this call so
        // body-template lookup can disambiguate when multiple libraries ship
        // templates for the same concept.
        String previousTag = CURRENT_LIBRARY_TAG.get();
        CURRENT_LIBRARY_TAG.set(targetLibraryTag == null ? "" : targetLibraryTag);
        // #1369: parametric expansions scope for composite CID decomposition.
        java.util.Map<String, ParametricExpansion> previousExpansions = CURRENT_EXPANSIONS.get();
        java.util.Map<String, ParametricExpansion> expMap = new java.util.HashMap<>();
        if (parametricExpansions != null) {
            for (ParametricExpansion exp : parametricExpansions) {
                expMap.put(exp.cid(), exp);
            }
        }
        CURRENT_EXPANSIONS.set(expMap);
        try {
            return emitStubInner(function, params, paramTypes, paramSortCids, returnType,
                    returnSortCid, conceptName, mode, modes, contract, sugarPluginJson,
                    transportedOp, termShapeJson, operandBindingsJson, isCrossLang);
        } finally {
            CURRENT_LIBRARY_TAG.set(previousTag);
            CURRENT_EXPANSIONS.set(previousExpansions);
        }
    }

    private static Realization emitStubInner(
            String function,
            List<String> params,
            List<String> paramTypes,
            List<String> paramSortCids,
            String returnType,
            String returnSortCid,
            String conceptName,
            String mode,
            List<String> modes,
            ContractPayload contract,
            List<String> sugarPluginJson,
            TransportedOperation transportedOp,
            String termShapeJson,
            String operandBindingsJson,
            boolean isCrossLang) {

        // Substrate-honest refusal: when this is a cross-language invocation
        // and ANY sort CID is empty (param or return), the source kit failed
        // to lift that type to a concept-hub identity. Emitting raw source
        // strings would leak kit-internal syntax. Refuse loudly with is_stub.
        //
        // isCrossLang is an EXPLICIT flag from the caller (RpcServer reads
        // it from the RPC params). Empty paramSortCids/returnSortCid in
        // legacy/same-lang context is absence-of-signal, NOT declared-gap.
        if (isCrossLang) {
            boolean anyEmpty = false;
            for (String cid : paramSortCids) {
                if (cid == null || cid.isEmpty()) { anyEmpty = true; break; }
            }
            if (anyEmpty || returnSortCid == null || returnSortCid.isEmpty()) {
                // Refuse: the substrate has no morphism for at least one
                // signature element. Caller (cmd_materialize) sees is_stub
                // and reports REFUSE — substrate gap surfaced, not concealed.
                String reason = "concept-hub gap in signature: source kit failed to lift "
                    + conceptName + " param/return types to concept-hub sort CIDs "
                    + "(paramSortCids=" + paramSortCids + " returnSortCid='" + returnSortCid + "'). "
                    + "Mint the missing substrate sort or refuse the boundary.";
                return new Realization("// REFUSE: " + reason, true, "{}", "[]", null);
            }
        }
        String className = snakeToPascal(function) + "Transported";
        String mappedReturn = resolveJavaType(returnType, returnSortCid, isCrossLang);
        if (mappedReturn == null) {
            String reason = "concept-hub gap: java kit has no morphism for return sort CID `"
                + returnSortCid + "` (concept=" + conceptName + "). "
                + "Mint the missing java realization or refuse the boundary.";
            return new Realization("// REFUSE: " + reason, true, "{}", "[]", null);
        }
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
            String sortCid = i < paramSortCids.size() ? paramSortCids.get(i) : "";
            String mapped = resolveJavaType(srcType, sortCid, isCrossLang);
            if (mapped == null) {
                String reason = "concept-hub gap: java kit has no morphism for param[" + i
                    + "] sort CID `" + sortCid + "` (concept=" + conceptName + ", param=" + name + "). "
                    + "Mint the missing java realization or refuse the boundary.";
                return new Realization("// REFUSE: " + reason, true, "{}", "[]", null);
            }
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

        String conceptCitationBlock = conceptCitationTagBlock(transportedOp, sugarPluginJson);
        boolean hasConceptCitationCarrier = !conceptCitationBlock.isBlank();
        Optional<RenderedBody> termShapeBody = hasConceptCitationCarrier
                ? Optional.empty()
                : renderTermShapeBody(termShapeJson, operandBindingsJson, params, paramTypes, returnType);
        Optional<RenderedBody> bodyTemplate = hasConceptCitationCarrier
                ? Optional.empty()
                : termShapeBody.or(() -> renderBodyTemplateFor(conceptName, params, mode));
        boolean isStub = bodyTemplate.isEmpty() && !hasConceptCitationCarrier;
        String bodyContent = hasConceptCitationCarrier
                ? conceptCitationBlock + "\n;"
                : bodyTemplate.map(RenderedBody::body)
                    .orElse("throw new UnsupportedOperationException(\"provekit-bind canonical: " + conceptName + "\");");
        Jcs.Json bodyLossRecord = hasConceptCitationCarrier
                ? conceptCitationCarrierLossRecord(transportedOp)
                : bodyTemplate.map(RenderedBody::lossRecord).orElse(Jcs.object());
        String observationWrapperEmissionRecord = null;
        if (!isStub && !hasConceptCitationCarrier) {
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
        // #1390: collect helpers from the matched body-template entry.
        List<String> helpers = bodyTemplate.map(RenderedBody::helpers).orElse(List.of());
        return new Realization(
                source,
                isStub,
                observedLossRecordJson(sugarEmissions, bodyLossRecord),
                usedSugarsJson(sugarEmissions),
                observationWrapperEmissionRecord,
                helpers);
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

    private static String conceptCitationTagBlock(
            TransportedOperation transportedOp,
            List<String> sugarPluginJson) {
        Jcs.Obj payload = conceptCitationPayload(transportedOp, sugarPluginJson);
        if (payload == null) {
            return "";
        }
        return "// provekit-concept: " + Jcs.encode(payload) + "\n"
                + "// provekit-concept-payload-cid: " + Jcs.cid(payload);
    }

    private static Jcs.Json conceptCitationCarrierLossRecord(TransportedOperation transportedOp) {
        String contribution = conceptCitationCarrierLossName(transportedOp);
        if (contribution.isBlank()) {
            return Jcs.object();
        }
        return Jcs.object(
                contribution, Jcs.object(
                        "args", Jcs.array(),
                        "head", Jcs.string("atomic"),
                        "name", Jcs.string(contribution)
                )
        );
    }

    private static String conceptCitationCarrierLossName(TransportedOperation transportedOp) {
        if (transportedOp == null) {
            return "";
        }
        if (conceptMatches("concept:addr", transportedOp.conceptName())
                || "addr".equals(transportedOp.operationKind())) {
            return "java-references-not-addresses";
        }
        if (conceptMatches("concept:deref", transportedOp.conceptName())
                || "deref".equals(transportedOp.operationKind())) {
            return "java-implicit-deref";
        }
        return "";
    }

    private static Jcs.Obj conceptCitationPayload(
            TransportedOperation transportedOp,
            List<String> sugarPluginJson) {
        if (transportedOp == null
                || !validCid(transportedOp.conceptCid())
                || !validCid(transportedOp.conceptSiteCid())
                || !validCid(transportedOp.lossRecordCid())
                || !validCid(transportedOp.shapeCid())
                || transportedOp.operationKind().isBlank()) {
            return null;
        }

        List<Jcs.Field> fields = new ArrayList<>();
        if (transportedOp.argsJcs() != null) {
            if (!(transportedOp.argsJcs() instanceof Jcs.Arr)) {
                return null;
            }
            fields.add(new Jcs.Field("args_jcs", transportedOp.argsJcs()));
            fields.add(new Jcs.Field("args_jcs_cid", Jcs.string(Jcs.cid(transportedOp.argsJcs()))));
        } else if (validCid(transportedOp.argsJcsCid())) {
            fields.add(new Jcs.Field("args_jcs_cid", Jcs.string(transportedOp.argsJcsCid())));
        } else {
            return null;
        }

        fields.add(new Jcs.Field("artifact_kind", Jcs.string("provekit-concept-citation-comment-sugar")));
        if (validCid(transportedOp.callsiteCid())) {
            fields.add(new Jcs.Field("callsite_cid", Jcs.string(transportedOp.callsiteCid())));
        } else if (transportedOp.callsiteCid() != null && !transportedOp.callsiteCid().isBlank()) {
            return null;
        }
        fields.add(new Jcs.Field("concept_cid", Jcs.string(transportedOp.conceptCid())));
        if (!transportedOp.conceptName().isBlank()) {
            fields.add(new Jcs.Field("concept_name", Jcs.string(transportedOp.conceptName())));
        }
        fields.add(new Jcs.Field("concept_site_cid", Jcs.string(transportedOp.conceptSiteCid())));
        fields.add(new Jcs.Field("emitted_by", Jcs.object(
                "kit_cid", Jcs.string(conceptCitationEmitterKitCid()),
                "kit_id", Jcs.string(conceptCitationKitId()),
                "kit_kind", Jcs.string("realize"),
                "target_language", Jcs.string("java"),
                "target_library_tag", Jcs.string(transportedOp.targetLibraryTag().isBlank()
                        ? "java"
                        : transportedOp.targetLibraryTag())
        )));
        fields.add(new Jcs.Field("loss_record_cid", Jcs.string(transportedOp.lossRecordCid())));
        fields.add(new Jcs.Field("operation_kind", Jcs.string(transportedOp.operationKind())));
        fields.add(new Jcs.Field("policy_cid", Jcs.string(conceptCitationPolicyCid(transportedOp))));
        fields.add(new Jcs.Field("schema_version", Jcs.string("1")));
        fields.add(new Jcs.Field("shape_cid", Jcs.string(transportedOp.shapeCid())));
        fields.add(new Jcs.Field("sugar_dict_cid", Jcs.string(conceptCitationSugarDictCid(transportedOp, sugarPluginJson))));
        fields.add(new Jcs.Field("term_position", Jcs.array(
                transportedOp.termPosition().stream().map(Jcs::integer).toList()
        )));
        return new Jcs.Obj(fields);
    }

    private static String conceptCitationPolicyCid(TransportedOperation transportedOp) {
        if (validCid(transportedOp.policyCid())) {
            return transportedOp.policyCid();
        }
        return Blake3.blake3_512("provekit-realize-java-core/default-concept-citation-policy"
                .getBytes(StandardCharsets.UTF_8));
    }

    private static String conceptCitationSugarDictCid(
            TransportedOperation transportedOp,
            List<String> sugarPluginJson) {
        if (validCid(transportedOp.sugarDictCid())) {
            return transportedOp.sugarDictCid();
        }
        if (sugarPluginJson != null) {
            for (String rawPlugin : sugarPluginJson) {
                try {
                    Jcs.Json parsed = Jcs.parse(rawPlugin);
                    if (!(parsed instanceof Jcs.Obj root)) continue;
                    Jcs.Json headerJson = root.get("header");
                    if (!(headerJson instanceof Jcs.Obj header)) continue;
                    String cid = header.stringFieldOrNull("cid");
                    if (validCid(cid)) {
                        return cid;
                    }
                } catch (IllegalArgumentException ignored) {
                }
            }
        }
        return Blake3.blake3_512("provekit-realize-java-core/concept-citation-comment-sugar-v1"
                .getBytes(StandardCharsets.UTF_8));
    }

    private static String conceptCitationKitId() {
        return "provekit-realize-java-core@0.1.0";
    }

    private static String conceptCitationEmitterKitCid() {
        return Blake3.blake3_512(conceptCitationKitId().getBytes(StandardCharsets.UTF_8));
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
        if (src == null) return "Object";
        // Strip rust borrow prefixes.
        String t = src.trim();
        if (t.startsWith("&mut ")) t = t.substring(5).trim();
        else if (t.startsWith("&mut")) t = t.substring(4).trim();
        if (t.startsWith("&")) t = t.substring(1).trim();
        // Slice-of-T: rust `[T]` → java. Bytes are common, fast-path
        // to byte[]. Other element types: java.util.List<T> for read-only
        // sequence semantics (matches what Vec<T>.iter().collect()
        // produces, so consumers don't see Vec↔array mismatches).
        if ("[u8]".equals(t)) return "byte[]";
        if (t.startsWith("[") && t.endsWith("]")) {
            String inner = t.substring(1, t.length() - 1).split(";")[0].trim();
            return "java.util.List<" + boxedType(mapSourceType(inner)) + ">";
        }
        // Unit type comes before tuple check.
        if ("()".equals(t)) return "void";
        // Rust tuple return `(A,B)` → java has no tuple; use Object[] as
        // a uniform carrier. Caller-side composes/destructures.
        if (t.startsWith("(") && t.endsWith(")")) {
            return "Object[]";
        }
        // Result<T,E>: rust's fallible return. Java equivalent is T +
        // exception for E. Emit just the success arm; bind sites that
        // propagate errors should throw RuntimeException.
        if (t.startsWith("Result<") && t.endsWith(">")) {
            String inner = t.substring(7, t.length() - 1).trim();
            int comma = findTopLevelComma(inner);
            String okType = comma > 0 ? inner.substring(0, comma).trim() : inner;
            return mapSourceType(okType);
        }
        // Option<T> → T (java null encodes None).
        if (t.startsWith("Option<") && t.endsWith(">")) {
            return mapSourceType(t.substring(7, t.length() - 1).trim());
        }
        // Vec<T> → java.util.List<T> (boxed for primitives).
        if (t.startsWith("Vec<") && t.endsWith(">")) {
            String inner = mapSourceType(t.substring(4, t.length() - 1).trim());
            return "java.util.List<" + boxedType(inner) + ">";
        }
        // Generic single-letter type param (A, T, K, V): erase to Object.
        if (t.matches("[A-Z]")) return "Object";
        return switch (t) {
            case "()" -> "void";
            case "i64", "u64", "usize", "isize" -> "long";
            case "i32", "u32" -> "int";
            case "i16", "u16" -> "short";
            case "i8", "u8" -> "byte";
            case "f64" -> "double";
            case "f32" -> "float";
            case "bool" -> "boolean";
            case "String", "str" -> "String";
            case "Value" -> "com.fasterxml.jackson.databind.JsonNode";
            case "Path", "PathBuf" -> "java.nio.file.Path";
            default -> t;
        };
    }

    /** Find top-level comma (not inside <> () [] {}). */
    private static int findTopLevelComma(String s) {
        int depth = 0;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '<', '(', '[', '{' -> depth++;
                case '>', ')', ']', '}' -> depth--;
                case ',' -> { if (depth == 0) return i; }
                default -> {}
            }
        }
        return -1;
    }

    /** Box a java primitive type for generic use. */
    private static String boxedType(String prim) {
        return switch (prim) {
            case "boolean" -> "Boolean";
            case "byte" -> "Byte";
            case "short" -> "Short";
            case "int" -> "Integer";
            case "long" -> "Long";
            case "float" -> "Float";
            case "double" -> "Double";
            case "char" -> "Character";
            default -> prim;
        };
    }

    /**
     * Substrate-honest type resolution: concept-hub sort CID → java syntax.
     *
     * Inverse of JavaBindLifter.javaTypeToConceptHubSortCid. The java kit's
     * internal knowledge of how its source syntax maps to substrate-canonical
     * concept-hub identities. Used in cross-language materialize where the
     * carrier's `param_sort_cids` (concept-hub CIDs) drive signature emission
     * instead of the source-language type strings.
     *
     * Returns null when the CID isn't recognized — caller falls back to
     * mapSourceType on the raw source string (legacy path).
     *
     * CIDs verified against menagerie/concept-shapes/catalog/sorts/.
     */
    static String mapConceptHubSortCidToJava(String cid) {
        if (cid == null || cid.isEmpty()) return null;
        // Primitive concept-hub sorts → java syntax.
        String primitive = switch (cid) {
            // concept:Bool
            case "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074" ->
                "boolean";
            // concept:Int — defaults to long for cross-language safety
            case "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58" ->
                "long";
            // concept:Float
            case "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57" ->
                "double";
            // concept:String
            case "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10" ->
                "String";
            // concept:Bytes
            case "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b" ->
                "byte[]";
            // concept:Unit — void return
            case "blake3-512:47682b09e5dba71f563db6249c6cb352f7d540986dc7f4cd8d4fb1aa6d9a503064033ee3eb9f36ee6f9e000f700f2f030ebfcfe2b2b8b7e81a345b0d56551f1b" ->
                "void";
            // concept:Json — JsonNode (Jackson)
            case "blake3-512:702064722b23410fde0d1fd7afac165bf5914441d67abe1e19d63b0e8fe8117296d2677cc721ad096b8b3bb82d178af699bf14fd70bfb18756c5bed6f4434108" ->
                "com.fasterxml.jackson.databind.JsonNode";
            default -> null;
        };
        if (primitive != null) return primitive;
        // #1369: composite parametric CIDs — decompose via the expansion table.
        ParametricExpansion exp = CURRENT_EXPANSIONS.get().get(cid);
        if (exp != null) {
            return resolveParametricToJava(exp);
        }
        return null;
    }

    /**
     * Parameterized morphism dispatch (#1369). Given a parametric application,
     * recursively resolve inner sort CIDs and emit the kit's idiomatic java
     * syntax. Ref<T> branches on inner T per the kit's realization choices:
     *   Ref<String> → StringBuilder
     *   Ref<Bytes>  → java.io.ByteArrayOutputStream
     *   Ref<other>  → AtomicReference<other>
     */
    private static String resolveParametricToJava(ParametricExpansion exp) {
        if (REF_T_CONSTRUCTOR_CID.equals(exp.constructorCid())) {
            if (exp.argCids().size() != 1) return null;
            String innerCid = exp.argCids().get(0);
            String innerJava = mapConceptHubSortCidToJava(innerCid);
            if (innerJava == null) return null;
            // Branch on inner T — java has type-specific mutable wrappers.
            return switch (innerJava) {
                case "String" -> "StringBuilder";
                case "byte[]" -> "java.io.ByteArrayOutputStream";
                default -> "java.util.concurrent.atomic.AtomicReference<" + innerJava + ">";
            };
        }
        if (LIST_T_CONSTRUCTOR_CID.equals(exp.constructorCid())) {
            if (exp.argCids().size() != 1) return null;
            String innerJava = mapConceptHubSortCidToJava(exp.argCids().get(0));
            if (innerJava == null) return null;
            // Java collections take REFERENCE types only — box primitives.
            String boxed = boxedJavaType(innerJava);
            return "java.util.List<" + boxed + ">";
        }
        // Unknown parametric constructor — substrate-honest gap signal.
        return null;
    }

    /** Box java primitive types for use as generic arguments. */
    private static String boxedJavaType(String t) {
        return switch (t) {
            case "boolean" -> "Boolean";
            case "byte" -> "Byte";
            case "short" -> "Short";
            case "int" -> "Integer";
            case "long" -> "Long";
            case "float" -> "Float";
            case "double" -> "Double";
            case "char" -> "Character";
            default -> t;
        };
    }

    /**
     * Substrate-honest type resolution wrapper. Prefers the concept-hub
     * sort CID when provided (cross-language materialize path); falls back
     * to source-language type translation when the CID is missing
     * (legacy carriers from before #1361 chunk 2 part B).
     */
    static String resolveJavaType(String sourceType, String conceptHubSortCid) {
        return resolveJavaType(sourceType, conceptHubSortCid, false);
    }

    /**
     * Substrate-honest type resolution. In cross-language mode, a non-empty
     * concept-hub sort CID that the java kit doesn't recognize is a real
     * substrate gap — return null so emitStubInner refuses loudly instead
     * of falling back on the source-language string (which would leak
     * source syntax into emitted java). In same-language / legacy mode,
     * fall back to mapSourceType when no concept-hub CID resolution exists.
     */
    static String resolveJavaType(String sourceType, String conceptHubSortCid, boolean isCrossLang) {
        String fromCid = mapConceptHubSortCidToJava(conceptHubSortCid);
        if (fromCid != null) return fromCid;
        if (isCrossLang && conceptHubSortCid != null && !conceptHubSortCid.isBlank()) {
            // Cross-language gap: the source kit lifted to a concept-hub CID
            // the java kit has no morphism for. Surface as null so the caller
            // refuses with is_stub instead of falling back to source syntax.
            return null;
        }
        return mapSourceType(sourceType);
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

    private record ShapeExpression(String text, String typeName) {}

    /** Cross-term function-return-type catalog set by the RPC entrypoint
     *  before invoking lowering. ShapeContext picks it up on construction
     *  via importThreadLocalCatalog(). Cleared between requests. */
    static final ThreadLocal<Map<String, String>> currentCallReturnTypes =
            ThreadLocal.withInitial(java.util.Map::of);

    private static final class ShapeContext {
        final List<String> params;
        final List<String> paramTypes;
        final String returnType;
        final Map<List<Integer>, String> operandBindings;
        final Set<String> definedSymbols = new TreeSet<>();
        /** Substrate-honest loss accumulator. Each silent approximation
         *  in the term-shape lowering APPENDS an entry here keyed by
         *  dimension name (e.g. "option-clone-erased"). emitStubInner
         *  merges into the realize plugin's observed_loss_record output.
         *  Empty = exact translation; non-empty = loudly-bounded-lossy. */
        final List<Jcs.Json> bodyApproximations = new ArrayList<>();

        void recordApproximation(String dimension, String detail) {
            bodyApproximations.add(Jcs.object(
                "kind", Jcs.string("approximation"),
                "dimension", Jcs.string(dimension),
                "detail", Jcs.string(detail)
            ));
        }
        /** Function-name → java return type. Built once from the
         *  named-term-doc; passed into each term's lowering so call
         *  expressions can pick up real return types instead of falling
         *  back to var inference. */
        Map<String, String> functionReturnTypes = java.util.Map.of();
        /** Raw rust tuple type captured from a recent `__provekit_tuple = call()`.
         *  Used by destructuring index-assigns to type each element. */
        String tupleSourceRawType = "";

        /** Reset per-call thread-local catalog into this context. */
        void importThreadLocalCatalog() {
            Map<String, String> tl = currentCallReturnTypes.get();
            if (tl != null && !tl.isEmpty()) this.functionReturnTypes = tl;
        }
        int nextLeaf = 0;
        int nextTemp = 0;
        String lastAssignedSymbol = "";

        ShapeContext(
                List<String> params,
                List<String> paramTypes,
                String returnType,
                Map<List<Integer>, String> operandBindings) {
            this.params = params;
            this.paramTypes = paramTypes;
            this.returnType = returnType;
            this.operandBindings = operandBindings;
            this.definedSymbols.addAll(params);
        }

        String lookupReturnType(String fnName) {
            String t = functionReturnTypes.get(fnName);
            return t == null ? "" : t;
        }

        ShapeExpression fallbackLeaf() {
            if (!params.isEmpty()) {
                int index = Math.min(nextLeaf, params.size() - 1);
                nextLeaf += 1;
                String type = index < paramTypes.size() ? mapSourceType(paramTypes.get(index)) : "";
                return new ShapeExpression(params.get(index), type);
            }
            nextLeaf += 1;
            return new ShapeExpression("0", "int");
        }

        String tempName() {
            String name = "__provekit_v" + nextTemp;
            nextTemp += 1;
            return name;
        }
    }

    private static Optional<RenderedBody> renderTermShapeBody(
            String termShapeJson,
            String operandBindingsJson,
            List<String> params,
            List<String> paramTypes,
            String returnType) {
        if (termShapeJson == null || termShapeJson.isBlank() || "{}".equals(termShapeJson.trim())) {
            return Optional.empty();
        }
        Jcs.Json parsed;
        try {
            parsed = Jcs.parse(termShapeJson);
        } catch (IllegalArgumentException e) {
            return Optional.empty();
        }
        if (!(parsed instanceof Jcs.Obj shape)) {
            return Optional.empty();
        }
        ShapeContext context = new ShapeContext(
                params,
                paramTypes,
                returnType,
                operandBindingMap(operandBindingsJson));
        context.importThreadLocalCatalog();
        Optional<String> body = lowerShapeBody(shape, context, List.of());
        if (body.isEmpty()) {
            Optional<ShapeExpression> expression = lowerShapeExpression(shape, context, List.of());
            if (expression.isEmpty() || expression.get().text().isBlank()) {
                return Optional.empty();
            }
            body = Optional.of("return " + expression.get().text() + ";");
        }
        // Aggregate the per-construct approximation entries accumulated
        // during term-shape lowering into the body's loss_record.
        // Substrate-honest: every silent lossy translation appended an
        // entry; this surface them to the realize plugin's caller via
        // observed_loss_record so the trichotomy
        // (exact/loudly-bounded-lossy/refuse) is observable.
        Jcs.Json lossRecord = buildApproximationLossRecord(context.bodyApproximations);
        return Optional.of(new RenderedBody(body.get(), lossRecord));
    }

    /** Aggregate context approximations into a loss-record keyed by
     *  dimension. Multiple approximations in the same dimension combine
     *  via an "or" formula (each is an independent occurrence). */
    private static Jcs.Json buildApproximationLossRecord(List<Jcs.Json> approximations) {
        if (approximations.isEmpty()) return Jcs.object();
        Map<String, List<Jcs.Json>> byDimension = new TreeMap<>();
        for (Jcs.Json entry : approximations) {
            if (!(entry instanceof Jcs.Obj obj)) continue;
            String dim = null;
            for (Jcs.Field f : obj.fields()) {
                if ("dimension".equals(f.key()) && f.value() instanceof Jcs.Str s) {
                    dim = s.value();
                }
            }
            if (dim == null) continue;
            byDimension.computeIfAbsent(dim, k -> new ArrayList<>()).add(entry);
        }
        List<Jcs.Field> outFields = new ArrayList<>();
        for (Map.Entry<String, List<Jcs.Json>> e : byDimension.entrySet()) {
            List<Jcs.Json> entries = e.getValue();
            if (entries.size() == 1) {
                outFields.add(new Jcs.Field(e.getKey(), entries.get(0)));
            } else {
                List<Jcs.Json> operands = new ArrayList<>(entries);
                outFields.add(new Jcs.Field(e.getKey(), Jcs.object(
                    "kind", Jcs.string("or"),
                    "operands", new Jcs.Arr(operands)
                )));
            }
        }
        return new Jcs.Obj(outFields);
    }

    private static Optional<String> lowerShapeBody(
            Jcs.Obj shape,
            ShapeContext context,
            List<Integer> position) {
        String conceptName = shapeConceptName(shape);
        List<Jcs.Obj> args = shapeArgs(shape);
        if (conceptMatches("concept:seq", conceptName) || "seq".equals(conceptName)) {
            List<String> lines = new ArrayList<>();
            for (int i = 0; i < args.size(); i++) {
                Jcs.Obj child = args.get(i);
                Optional<String> childBody = lowerShapeBody(child, context, appendPosition(position, i));
                if (childBody.isPresent()) {
                    if (!childBody.get().isBlank()) {
                        lines.add(childBody.get());
                    }
                    continue;
                }
                Optional<ShapeExpression> expression = lowerShapeExpression(child, context, appendPosition(position, i));
                if (expression.isEmpty() || expression.get().text().isBlank()) {
                    // Partial lowering: emit a TODO comment for the unhandled
                    // child instead of aborting the entire body. This lets
                    // simpler statements in the seq translate cleanly even
                    // when later statements have constructs the lower
                    // vocabulary can't yet handle. Substrate-honest: surface
                    // the gap inline; don't paper over with a stub return.
                    String childConcept = shapeConceptName(child);
                    lines.add("// TODO(lower): un-lowered " + (childConcept.isBlank() ? "leaf" : childConcept));
                    continue;
                }
                String text = expression.get().text();
                // Expression in statement position. If it's a function call
                // (or method call) with no obvious result-use, emit as a
                // bare statement (`call();`). The artifact-binding form
                // (`T tmp = call();`) is incorrect for void-returning
                // functions and produces uncompilable java. Heuristic:
                // emit as bare statement; assign-to-artifact only if the
                // expression has no parens (rare in practice).
                if (text.endsWith(")") || text.endsWith("]") || text.endsWith("\"")) {
                    String inner = text;
                    while (inner.startsWith("(") && inner.endsWith(")") &&
                           matchingOuterParens(inner)) {
                        inner = inner.substring(1, inner.length() - 1).trim();
                    }
                    // Tail-expression detection: when this is the LAST
                    // child of the seq AND we're at the function root
                    // AND the function returns non-void, emit `return X;`
                    // instead of a bare statement. This matches rust's
                    // implicit-tail-return convention.
                    boolean isLast = (i == args.size() - 1);
                    boolean nonVoid = !"void".equals(mapSourceType(context.returnType));
                    if (isLast && position.isEmpty() && nonVoid) {
                        lines.add("return " + inner + ";");
                    } else {
                        String stmt = inner.endsWith(";") ? inner : inner + ";";
                        lines.add(stmt);
                    }
                } else if (isIdentifier(text)) {
                    // Bare identifier as the tail expression — no temp
                    // needed; use the symbol directly as the implicit
                    // return value.
                    context.lastAssignedSymbol = text;
                } else {
                    String temp = context.tempName();
                    context.definedSymbols.add(temp);
                    context.lastAssignedSymbol = temp;
                    // Use the value's typeName when known; falls back to
                    // var inference for unknown types.
                    String tempType = expression.get().typeName();
                    if (tempType == null || tempType.isBlank() || "Object".equals(tempType)) {
                        tempType = "";  // localDeclaration emits `var`
                    }
                    lines.add(localDeclaration(tempType, temp, text, false));
                }
            }
            // Only emit implicit return at the OUTERMOST seq (the function
            // body itself). Nested seqs (inside for/while/match arms) must
            // NOT inject a return — they're statement-blocks, not the
            // function tail. Position == empty list means root.
            // Only emit implicit return when:
            // - position is empty (function root)
            // - non-void return type
            // - NO existing return statement appears in any line (search
            //   substring not just prefix to catch nested returns inside
            //   if/else/switch blocks the match handler emitted)
            // - we have a tail symbol to return
            boolean anyReturn = lines.stream().anyMatch(line ->
                line.contains("return ") || line.contains("throw "));
            if (position.isEmpty()
                    && !"void".equals(mapSourceType(context.returnType))
                    && !anyReturn
                    && !context.lastAssignedSymbol.isBlank()) {
                String sym = context.lastAssignedSymbol;
                // If the symbol's declared type was StringBuilder but the
                // function returns String, call .toString() to coerce.
                // (Rust idiom: build a String via String + push_str/push
                // which maps to StringBuilder in java.)
                String fnReturn = mapSourceType(context.returnType);
                boolean needsToString = "String".equals(fnReturn)
                        && lines.stream().anyMatch(line ->
                            line.contains("StringBuilder " + sym + " ="));
                lines.add("return " + sym + (needsToString ? ".toString()" : "") + ";");
            }
            return Optional.of(String.join("\n", lines));
        }
        // concept:assign comes in two shapes: 2-arg (target, value) for
        // standard let, 3-arg (target, value, mutability_leaf) for `let mut`.
        // The third arg is just a marker — semantically equivalent for java
        // (where all locals are mutable). Accept both.
        if (conceptMatches("concept:assign", conceptName) && args.size() >= 2 && args.size() <= 3) {
            // Special-case `let target = match X { ... }` where arms have
            // control-flow (return/break/continue) bodies. The match value
            // can't be lowered as a single expression; emit if-else chain
            // that ASSIGNS the target per arm. Detect: args[1] is
            // concept:match.
            Optional<ShapeExpression> target0 = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            if (target0.isPresent() && isIdentifier(target0.get().text())
                    && conceptMatches("concept:match", shapeConceptName(args.get(1)))) {
                String matchAsStmts = lowerMatchAsAssignmentTo(target0.get().text(), args.get(1), context, appendPosition(position, 1));
                if (matchAsStmts != null) {
                    context.definedSymbols.add(target0.get().text());
                    context.lastAssignedSymbol = target0.get().text();
                    return Optional.of(matchAsStmts);
                }
            }
            Optional<ShapeExpression> target = target0;
            Optional<ShapeExpression> value = lowerShapeExpression(args.get(1), context, appendPosition(position, 1));
            if (target.isEmpty() || value.isEmpty() || !isIdentifier(target.get().text())) {
                return Optional.empty();
            }
            String name = target.get().text();
            boolean alreadyDefined = context.definedSymbols.contains(name);
            context.definedSymbols.add(name);
            context.lastAssignedSymbol = name;
            // Use the VALUE's inferred type for the declaration. If
            // unknown, prefer `var` (java 10+ infers from RHS) over
            // falling back to the enclosing function's return type —
            // that fallback was producing wrong-type declarations like
            // `String raw = blake3_512_of(bytes);` where blake3_512_of
            // returns byte[].
            String declType = value.get().typeName();
            if (declType == null || declType.isBlank()) {
                declType = "";  // localDeclaration emits `var` for blank.
            }
            // Substrate-honest tuple destructure: when assigning from a
            // synthetic tuple index (__provekit_tuple[N]) AND we know the
            // tuple-producing function's return type, use the element type
            // for the declaration so downstream method calls don't fail
            // on Object.
            String valueText = value.get().text();
            // Strip outer parens that may have been added (e.g. (__provekit_tuple[0])).
            String stripped = valueText.trim();
            while (stripped.startsWith("(") && stripped.endsWith(")")
                    && matchingOuterParens(stripped)) {
                stripped = stripped.substring(1, stripped.length() - 1).trim();
            }
            if (stripped.startsWith("__provekit_tuple[") && stripped.endsWith("]")) {
                String tupleType = context.tupleSourceRawType;
                if (tupleType != null && !tupleType.isBlank()) {
                    String idxStr = stripped.substring("__provekit_tuple[".length(),
                                                       stripped.length() - 1).trim();
                    try {
                        int idx = Integer.parseInt(idxStr);
                        String elemType = tupleElementType(tupleType, idx);
                        if (elemType != null) {
                            String javaElem = mapSourceType(elemType);
                            return Optional.of(javaElem + " " + name + " = (" + javaElem + ") " + stripped + ";");
                        }
                    } catch (NumberFormatException ignore) {}
                }
            }
            // Detect the seed assign `__provekit_tuple = handle_line(...)` —
            // record the raw tuple type so subsequent index-assigns can
            // resolve element types.
            if ("__provekit_tuple".equals(name)) {
                String fnName = extractCalledFnName(valueText);
                if (fnName != null) {
                    String raw = context.functionReturnTypes.getOrDefault(fnName, "");
                    String fnRetStripped = raw.trim();
                    if (fnRetStripped.startsWith("&mut ")) fnRetStripped = fnRetStripped.substring(5).trim();
                    else if (fnRetStripped.startsWith("&")) fnRetStripped = fnRetStripped.substring(1).trim();
                    if (fnRetStripped.startsWith("(") && fnRetStripped.endsWith(")")) {
                        context.tupleSourceRawType = fnRetStripped;
                    } else {
                        context.tupleSourceRawType = "";
                    }
                }
            }
            return Optional.of(localDeclaration(declType, name, value.get().text(), alreadyDefined));
        }
        if (conceptMatches("concept:return", conceptName)) {
            if (args.isEmpty()) {
                return Optional.of("return;");
            }
            Optional<ShapeExpression> value = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            return value.map(shapeExpression -> {
                String text = shapeExpression.text().trim();
                // If value is already a throw/return statement (from Err
                // translation), use it as-is — `return throw new ...;`
                // is invalid.
                if (text.startsWith("throw ") || text.startsWith("return ")) {
                    return text.endsWith(";") ? text : text + ";";
                }
                return "return " + shapeExpression.text() + ";";
            });
        }
        if (conceptMatches("concept:conditional", conceptName) && args.size() == 3) {
            Optional<ShapeExpression> condition = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            Optional<String> thenBody = lowerShapeBranchBody(args.get(1), context, appendPosition(position, 1));
            Optional<String> elseBody = lowerShapeBranchBody(args.get(2), context, appendPosition(position, 2));
            if (condition.isEmpty() || thenBody.isEmpty() || elseBody.isEmpty()) {
                return Optional.empty();
            }
            // Java requires parens around the WHOLE condition. Always
            // wrap with outer parens — operators like (a)!=(b) start with
            // `(` but aren't a single parenthesized expression.
            String condText = "(" + condition.get().text() + ")";
            return Optional.of("if " + condText + " {\n"
                    + indentBlock(thenBody.get()) + "\n"
                    + "} else {\n"
                    + indentBlock(elseBody.get()) + "\n"
                    + "}");
        }
        if (conceptMatches("concept:comment", conceptName) && !args.isEmpty()) {
            Jcs.Json value = args.get(0).get("value");
            if (value instanceof Jcs.Str s) {
                return Optional.of("// " + s.value().replace("\n", "\n// "));
            }
        }
        if (conceptMatches("concept:skip", conceptName) && args.isEmpty()) {
            return Optional.of("");
        }
        // Lift vocabulary parity: rust walk_rpc emits these concepts; java
        // lower must accept them or translation bails on the first
        // unrecognized construct in a body.
        // concept:while-let(var_leaf, value_expr, body) — rust's `while let
        // Some(var) = value` pattern. Java: `while ((var = value) != null) { body }`.
        // The var is declared in the enclosing scope so the loop can refer
        // to it; the loop body can also assume var is non-null.
        if (conceptMatches("concept:while-let", conceptName) && args.size() == 3) {
            String varName = args.get(0).stringFieldOrNull("text");
            if (varName == null || varName.isBlank()) return Optional.empty();
            varName = varName.trim();
            Optional<ShapeExpression> value = lowerShapeExpression(args.get(1), context, appendPosition(position, 1));
            if (value.isEmpty()) return Optional.empty();
            context.definedSymbols.add(varName);
            Optional<String> body = lowerShapeBranchBody(args.get(2), context, appendPosition(position, 2));
            if (body.isEmpty()) return Optional.empty();
            String valueType = value.get().typeName();
            String decl = (valueType == null || valueType.isBlank() || "Object".equals(valueType)) ? "var" : valueType;
            // Java: declare var outside loop, assign + null-check in cond.
            return Optional.of(
                decl + " " + varName + " = " + value.get().text() + ";\n" +
                "while (" + varName + " != null) {\n" +
                indentBlock(body.get()) + "\n" +
                "    " + varName + " = " + value.get().text() + ";\n" +
                "}"
            );
        }
        if (conceptMatches("concept:while", conceptName) && args.size() == 2) {
            Optional<ShapeExpression> cond = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            Optional<String> body = lowerShapeBranchBody(args.get(1), context, appendPosition(position, 1));
            if (cond.isEmpty() || body.isEmpty()) return Optional.empty();
            return Optional.of("while (" + cond.get().text() + ") {\n"
                    + indentBlock(body.get()) + "\n"
                    + "}");
        }
        if (conceptMatches("concept:for-each", conceptName) && args.size() == 3) {
            // args[0]: loop variable leaf (symbol). args[1]: iterable. args[2]: body.
            String varName = args.get(0).stringFieldOrNull("text");
            if (varName == null || varName.isBlank()) {
                Optional<ShapeExpression> v = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
                if (v.isEmpty()) return Optional.empty();
                varName = v.get().text();
            }
            // Strip rust ref-pattern prefix from loop var: `& b` → `b`,
            // `mut b` → `b`, `& mut b` → `b`. The iterable's deref is also
            // implicit in java collections.
            varName = varName.replaceAll("^\\s*(&\\s*mut\\s+|&\\s*|mut\\s+)", "").trim();
            Optional<ShapeExpression> iter = lowerShapeExpression(args.get(1), context, appendPosition(position, 1));
            if (iter.isEmpty()) return Optional.empty();
            context.definedSymbols.add(varName);
            Optional<String> body = lowerShapeBranchBody(args.get(2), context, appendPosition(position, 2));
            if (body.isEmpty()) return Optional.empty();
            return Optional.of("for (var " + varName + " : " + iter.get().text() + ") {\n"
                    + indentBlock(body.get()) + "\n"
                    + "}");
        }
        if (conceptMatches("concept:break", conceptName)) {
            if (args.isEmpty()) return Optional.of("break;");
            // break with value (rust's `break expr` in a loop) -> standard
            // java has no equivalent at statement scope; emit a comment.
            return Optional.of("break; // TODO: rust break-with-value not directly representable in java");
        }
        if (conceptMatches("concept:continue", conceptName)) {
            return Optional.of("continue;");
        }
        // concept:match in STATEMENT position: emit if-else if-else chain
        // with `return X;` per arm. Avoids the Supplier-lambda scope issue
        // (let bindings inside arms can't reach outer scope when wrapped
        // in a lambda). Each arm body is wrapped as `return body;` since
        // match in tail position IS the function's return value.
        if (conceptMatches("concept:match", conceptName) && args.size() >= 2) {
            Optional<ShapeExpression> scrut = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            if (scrut.isEmpty()) return Optional.empty();
            String scrutVar = context.tempName();
            context.definedSymbols.add(scrutVar);
            StringBuilder out = new StringBuilder();
            out.append("var ").append(scrutVar).append(" = ").append(scrut.get().text()).append(";\n");
            boolean defaultSeen = false;
            int armIdx = 0;
            for (int i = 1; i < args.size(); i++) {
                Jcs.Obj arm = args.get(i);
                if (!conceptMatches("concept:match-arm", shapeConceptName(arm))) continue;
                List<Jcs.Obj> armArgs = shapeArgs(arm);
                if (armArgs.size() < 2) continue;
                String patternText = armArgs.get(0).stringFieldOrNull("text");
                if (patternText == null) patternText = "";
                patternText = patternText.trim();
                String boundVar = bindingFromPattern(patternText);
                if (boundVar != null) context.definedSymbols.add(boundVar);
                String cond = patternToCondition(patternText, scrutVar);
                Optional<ShapeExpression> bodyExpr = lowerShapeExpression(armArgs.get(1), context, appendPosition(position, i));
                if (bodyExpr.isEmpty()) return Optional.empty();
                String bodyText = bodyExpr.get().text();
                if (boundVar != null) {
                    String sub = patternHasNestedBinding(patternText)
                            ? "String.valueOf(" + scrutVar + ")"
                            : scrutVar;
                    bodyText = replaceIdentifier(bodyText, boundVar, sub);
                }
                if ("true".equals(cond)) {
                    if (armIdx == 0) {
                        out.append("return ").append(bodyText).append(";");
                    } else {
                        out.append(" else {\n    return ").append(bodyText).append(";\n}");
                    }
                    defaultSeen = true;
                    break;
                }
                if (armIdx == 0) out.append("if (").append(cond).append(") {\n");
                else out.append(" else if (").append(cond).append(") {\n");
                out.append("    return ").append(bodyText).append(";\n}");
                armIdx++;
            }
            if (!defaultSeen && armIdx > 0) {
                out.append(" else {\n    return null;\n}");
            }
            return Optional.of(out.toString());
        }
        return Optional.empty();
    }

    private static Optional<String> lowerShapeBranchBody(
            Jcs.Obj shape,
            ShapeContext context,
            List<Integer> position) {
        Optional<String> body = lowerShapeBody(shape, context, position);
        if (body.isPresent()) {
            return body;
        }
        Optional<ShapeExpression> expression = lowerShapeExpression(shape, context, position);
        return expression.map(shapeExpression -> "return " + shapeExpression.text() + ";");
    }

    private static Optional<ShapeExpression> lowerShapeExpression(
            Jcs.Obj shape,
            ShapeContext context,
            List<Integer> position) {
        String conceptName = shapeConceptName(shape);
        if (conceptName.isBlank()) {
            return Optional.of(shapeLeafExpression(shape, context, position));
        }
        List<Jcs.Obj> args = shapeArgs(shape);
        if (conceptMatches("concept:seq", conceptName) || "seq".equals(conceptName)) {
            Optional<String> body = lowerShapeBody(shape, context, position);
            if (body.isEmpty()) {
                return Optional.empty();
            }
            if (!context.lastAssignedSymbol.isBlank()) {
                return Optional.of(new ShapeExpression(context.lastAssignedSymbol, mapSourceType(context.returnType)));
            }
            return Optional.empty();
        }
        // concept:literal — the lifter wraps literal values as
        // concept:literal{value: <literal>, sort: <CID>}. Lower by emitting
        // the literal directly. Strings come with quotes already in value.
        if (conceptMatches("concept:literal", conceptName)) {
            Jcs.Json valueJson = shape.get("value");
            if (valueJson != null) {
                return Optional.of(literalTerm(valueJson));
            }
            return Optional.empty();
        }
        // concept:match(scrutinee, arm1, arm2, ...). Each arm is
        // concept:match-arm(pattern_leaf, body). Lower as a chained
        // ternary in expression position. The first arm whose pattern
        // matches wins; rust's `_` wildcard becomes the else.
        //
        // Patterns supported (best-effort): `_` (wildcard, always true),
        // `Some(v)` (treats scrutinee as a JsonNode that may be null —
        // binds v to the scrutinee value), `None` (scrutinee is null),
        // bare identifiers (always true, binds to scrutinee). Guards
        // (`Some(v) if cond`) are recognized in the pattern text.
        if (conceptMatches("concept:match", conceptName) && args.size() >= 2) {
            Optional<ShapeExpression> scrut = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            if (scrut.isEmpty()) return Optional.empty();
            String scrutText = scrut.get().text();
            // If ALL non-wildcard arm patterns are string literals, emit
            // java switch expression (java 14+). Cleaner than ternary
            // lambda — keeps each arm at its own statement scope,
            // sidesteps the Supplier-lambda issue where let-bindings
            // inside one arm can't reach outer code.
            if (allPatternsStringLiteral(args)) {
                StringBuilder sw = new StringBuilder();
                sw.append("switch (").append(scrutText).append(") {");
                String defaultArm = null;
                for (int i = 1; i < args.size(); i++) {
                    Jcs.Obj arm = args.get(i);
                    if (!conceptMatches("concept:match-arm", shapeConceptName(arm))) continue;
                    List<Jcs.Obj> armArgs = shapeArgs(arm);
                    if (armArgs.size() < 2) continue;
                    String patternText = (armArgs.get(0).stringFieldOrNull("text") == null
                            ? "" : armArgs.get(0).stringFieldOrNull("text")).trim();
                    Optional<ShapeExpression> body = lowerShapeExpression(armArgs.get(1), context, appendPosition(position, i));
                    if (body.isEmpty()) return Optional.empty();
                    if ("_".equals(patternText)) {
                        defaultArm = body.get().text();
                    } else if (isStringLiteral(patternText)) {
                        sw.append(" case ").append(patternText).append(" -> ").append(body.get().text()).append(";");
                    }
                }
                sw.append(" default -> ").append(defaultArm != null ? defaultArm : "null").append("; }");
                return Optional.of(new ShapeExpression(sw.toString(), mapSourceType(context.returnType)));
            }
            // Use a fresh variable for the scrutinee so it isn't re-evaluated.
            String scrutVar = context.tempName();
            context.definedSymbols.add(scrutVar);
            // Build a chain: cond1 ? body1 : (cond2 ? body2 : default).
            StringBuilder chain = new StringBuilder();
            int armCount = 0;
            int openParens = 0;
            boolean defaultEmitted = false;
            for (int i = 1; i < args.size(); i++) {
                Jcs.Obj arm = args.get(i);
                if (!conceptMatches("concept:match-arm", shapeConceptName(arm))) continue;
                List<Jcs.Obj> armArgs = shapeArgs(arm);
                if (armArgs.size() < 2) continue;
                String patternText = armArgs.get(0).stringFieldOrNull("text");
                if (patternText == null) patternText = "";
                patternText = patternText.trim();
                // For binding patterns like `Some(v)`, expose v as the scrut value.
                String boundVar = bindingFromPattern(patternText);
                if (boundVar != null) {
                    context.definedSymbols.add(boundVar);
                }
                Optional<ShapeExpression> body = lowerShapeExpression(armArgs.get(1), context, appendPosition(position, i));
                if (body.isEmpty()) return Optional.empty();
                String bodyText = body.get().text();
                String cond = patternToCondition(patternText, scrutVar);
                if (boundVar != null) {
                    String sub = patternHasNestedBinding(patternText)
                            ? "String.valueOf(" + scrutVar + ")"
                            : scrutVar;
                    bodyText = replaceIdentifier(bodyText, boundVar, sub);
                }
                if ("true".equals(cond)) {
                    if (armCount == 0) {
                        return Optional.of(new ShapeExpression(
                                "(((java.util.function.Supplier<" + boxedType(mapSourceType(context.returnType).equals("void") ? "Object" : mapSourceType(context.returnType)) + ">) () -> { var " + scrutVar + " = " + scrutText + "; return " + bodyText + "; }).get())",
                                mapSourceType(context.returnType)));
                    }
                    // Default arm after at least one conditional: emit as
                    // ternary else branch.
                    chain.append(" : ").append(bodyText);
                    armCount++;
                    defaultEmitted = true;
                    break;
                } else {
                    if (armCount > 0) {
                        chain.append(" : (");
                        openParens++;
                    }
                    chain.append(cond).append(" ? ").append(bodyText);
                    armCount++;
                }
            }
            // If no default arm, fall through to a synthetic null/zero.
            if (armCount == 0) return Optional.empty();
            // Was there a wildcard? If chain doesn't end with a default,
            // append `: null` so the ternary is well-formed.
            // Heuristic: last arm wasn't a "true" cond means we need a fallback.
            // Look at the last char of chain; if it ends with bodyText (not closed
            // with " : default"), append a default.
            // Simpler: always append " : null" for the innermost level and one
            // closing paren PER non-default arm after the first.
            // Rust match is exhaustive; java's ternary needs an else.
            // Substrate-honest: if rust source had no wildcard, the
            // fallback path is unreachable per rust semantics — emit
            // a panic at runtime + loss_record entry rather than
            // silently fabricate `: null` (which the source did not
            // authorize as a return value).
            if (!defaultEmitted) {
                context.recordApproximation(
                    "match-expression-synthesized-default",
                    "Rust match expression had no wildcard; java ternary requires else. "
                        + "Emitted a panic for the unreachable branch.");
                chain.append(" : (").append(boxedType(mapSourceType(context.returnType).equals("void") ? "Object" : mapSourceType(context.returnType)))
                     .append(") (Object) ((java.util.function.Supplier<Object>)() -> { throw new RuntimeException(\"exhaustive match: no arm matched\"); }).get()");
            }
            for (int k = 0; k < openParens; k++) chain.append(")");
            String supplierType = boxedType(mapSourceType(context.returnType).equals("void") ? "Object" : mapSourceType(context.returnType));
            String wrapped = "(((java.util.function.Supplier<" + supplierType + ">) () -> { var " + scrutVar + " = " + scrutText + "; return " + chain + "; }).get())";
            return Optional.of(new ShapeExpression(wrapped, mapSourceType(context.returnType)));
        }
        // concept:macro-call(macro_name_leaf, body_leaf). Handle known macros
        // by parsing the raw text body. Unknown macros bail to Empty.
        if (conceptMatches("concept:macro-call", conceptName) && args.size() >= 2) {
            String macroName = args.get(0).stringFieldOrNull("text");
            String macroBody = args.get(1).stringFieldOrNull("text");
            if (macroName == null || macroBody == null) return Optional.empty();
            if ("json".equals(macroName)) {
                String emitted = emitJsonMacro(macroBody, context);
                if (emitted == null) return Optional.empty();
                return Optional.of(new ShapeExpression(emitted, "com.fasterxml.jackson.databind.JsonNode"));
            }
            if ("format".equals(macroName)) {
                String emitted = emitFormatMacro(macroBody, context);
                if (emitted == null) return Optional.empty();
                return Optional.of(new ShapeExpression(emitted, "String"));
            }
            return Optional.empty();
        }
        // #1390+ vocabulary: concept:call with a method-leaf at args[1] is a
        // method call. Handle BEFORE the generic arg-lowering loop, because
        // the method leaf ({kind: "method", text: ...}) doesn't lower as a
        // normal expression and would fail the loop.
        if (conceptMatches("concept:call", conceptName) && args.size() >= 2) {
            Jcs.Json maybeMethod = args.get(1).get("kind");
            if (maybeMethod instanceof Jcs.Str ks && "method".equals(ks.value())) {
                Optional<ShapeExpression> receiver = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
                if (receiver.isEmpty()) return Optional.empty();
                String methodName = args.get(1).stringFieldOrNull("text");
                if (methodName == null || methodName.isBlank()) return Optional.empty();
                List<String> callArgs = new ArrayList<>();
                for (int i = 2; i < args.size(); i++) {
                    Optional<ShapeExpression> a = lowerShapeExpression(args.get(i), context, appendPosition(position, i));
                    if (a.isEmpty()) return Optional.empty();
                    callArgs.add(a.get().text());
                }
                String joined = String.join(", ", callArgs);
                // Translate common rust methods to java equivalents.
                String javaMethod = mapRustMethodToJava(methodName);
                String javaArgs = joined;
                // .as_bytes() in rust → .getBytes(StandardCharsets.UTF_8) in java.
                if ("as_bytes".equals(methodName)) {
                    return Optional.of(new ShapeExpression(
                            receiver.get().text() + ".getBytes(java.nio.charset.StandardCharsets.UTF_8)",
                            "byte[]"));
                }
                // .into() / .clone() / .cloned() on a value: no-op for
                // most java types. Drop the call (identity). Record as
                // approximation — rust .cloned() on Option<&T> would
                // clone the inner; we're trusting java reference-sharing
                // semantics make this safe, which is unverified for
                // arbitrary T.
                if ("into".equals(methodName) || "clone".equals(methodName)
                        || "cloned".equals(methodName)) {
                    context.recordApproximation(
                        "option-clone-erased",
                        "Rust ." + methodName + "() lowered as identity on '"
                            + receiver.get().text() + "'; relies on java reference-sharing");
                    return Optional.of(new ShapeExpression(receiver.get().text(), receiver.get().typeName()));
                }
                // .iter() on a JsonNode (rust Vec<Value>.iter()) →
                // StreamSupport.stream over spliterator. Caller's
                // .filter_map / .collect continue the pipeline.
                if ("iter".equals(methodName) && callArgs.isEmpty()) {
                    return Optional.of(new ShapeExpression(
                            "java.util.stream.StreamSupport.stream(" + receiver.get().text()
                            + ".spliterator(), false)",
                            "java.util.stream.Stream"));
                }
                // .filter_map(fn) on a Stream — semantically equivalent
                // to .map(fn).filter(Objects::nonNull) in java.
                if ("filter_map".equals(methodName) && callArgs.size() == 1) {
                    String fn = callArgs.get(0);
                    String mapper;
                    if (fn.contains("->")) {
                        // Type the lambda explicitly as
                        // Function<JsonNode, Object> so that the lambda
                        // body's method calls resolve against JsonNode
                        // (the common case for our cross-platform crate).
                        mapper = "((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, Object>)(" + fn + "))";
                    } else {
                        mapper = "(" + fn + ")";
                    }
                    return Optional.of(new ShapeExpression(
                            receiver.get().text() + ".map(" + mapper + ").filter(java.util.Objects::nonNull)",
                            "java.util.stream.Stream"));
                }
                // .collect() on a Stream → .collect(Collectors.toList()).
                if ("collect".equals(methodName) && callArgs.isEmpty()) {
                    return Optional.of(new ShapeExpression(
                            receiver.get().text() + ".collect(java.util.stream.Collectors.toList())",
                            "java.util.List"));
                }
                // .ok_or_else(closure) on Option<T> in rust: Some(x) → Ok(x),
                // None → Err(closure()). Java equivalent on a nullable T:
                // (recv != null ? recv : throw_via_supplier(closure)). We
                // can't `throw` inline in an expression position; emit a
                // null-fallback that returns the closure value (already
                // a String via String.valueOf). Caller can wrap in
                // Objects.requireNonNull at the binding level if needed.
                if ("ok_or_else".equals(methodName) && callArgs.size() == 1) {
                    context.recordApproximation(
                        "result-flattened-to-throw",
                        "Rust .ok_or_else(closure) lowered as throw-on-null. "
                            + "Callers expecting Result<T,E> cannot recover the error path.");
                    String fn = callArgs.get(0);
                    String fnInvoked;
                    if (fn.contains("->")) {
                        // Lambda with no param (() -> body) — extract body.
                        java.util.regex.Matcher m = java.util.regex.Pattern.compile(
                            "^\\s*\\(\\s*\\)\\s*->\\s*(.+)$"
                        ).matcher(fn);
                        if (m.find()) {
                            fnInvoked = m.group(1).trim();
                        } else {
                            fnInvoked = "((java.util.function.Supplier)(" + fn + ")).get()";
                        }
                    } else {
                        fnInvoked = "((java.util.function.Supplier)(" + fn + ")).get()";
                    }
                    return Optional.of(new ShapeExpression(
                            "java.util.Objects.requireNonNullElseGet(" + receiver.get().text()
                            + ", () -> { throw new RuntimeException(String.valueOf(" + fnInvoked + ")); })",
                            ""));
                }
                // .insert(x) on rust BTreeSet/HashSet → .add(x) returning boolean.
                // .insert(k, v) on rust BTreeMap/HashMap → .put(k, v) returning prev.
                // Disambiguate by argument count.
                if ("insert".equals(methodName)) {
                    if (callArgs.size() == 1) {
                        return Optional.of(new ShapeExpression(
                                receiver.get().text() + ".add(" + callArgs.get(0) + ")",
                                "boolean"));
                    }
                    if (callArgs.size() == 2) {
                        String v = callArgs.get(1).trim();
                        boolean isJsonNodeValue = v.contains("valueToTree")
                                || v.contains("MAPPER.create")
                                || v.contains("MAPPER.nullNode");
                        if (isJsonNodeValue) {
                            // .set is on ObjectNode, not JsonNode. Cast
                            // the receiver to ObjectNode so the method
                            // resolves.
                            return Optional.of(new ShapeExpression(
                                    "((com.fasterxml.jackson.databind.node.ObjectNode) " + receiver.get().text() + ")"
                                    + ".set(" + callArgs.get(0) + ", " + callArgs.get(1) + ")",
                                    ""));
                        }
                        return Optional.of(new ShapeExpression(
                                receiver.get().text() + ".put(" + callArgs.get(0) + ", " + callArgs.get(1) + ")",
                                ""));
                    }
                }
                // .push(x) is ambiguous: rust Vec.push → java List.add;
                // rust String.push(char) → java StringBuilder.append. The
                // arg's surface gives the cleanest signal — a (char) cast
                // means StringBuilder. Otherwise default to List.add.
                if ("push".equals(methodName) && callArgs.size() == 1) {
                    String arg = callArgs.get(0).trim();
                    if (arg.startsWith("(char)") || arg.startsWith("(char ")) {
                        return Optional.of(new ShapeExpression(
                                receiver.get().text() + ".append(" + arg + ")",
                                ""));
                    }
                    return Optional.of(new ShapeExpression(
                            receiver.get().text() + ".add(" + arg + ")",
                            "boolean"));
                }
                // .unwrap() / .unwrap_or(x) on Option/Result: java has the
                // value directly (loss = null-means-None). Pass through.
                if ("unwrap".equals(methodName)) {
                    return Optional.of(new ShapeExpression(
                            receiver.get().text(),
                            ""));  // unknown call return; var inference
                }
                if ("unwrap_or".equals(methodName) && callArgs.size() == 1) {
                    return Optional.of(new ShapeExpression(
                            "(" + receiver.get().text() + " != null ? " + receiver.get().text() + " : " + callArgs.get(0) + ")",
                            ""));  // unknown call return; var inference
                }
                // .and_then(f) on Option: Some(v) → f(v); None → None.
                // In java: (recv != null ? f(recv) : null).
                if ("and_then".equals(methodName) && callArgs.size() == 1) {
                    String fn = callArgs.get(0);
                    // Lambda fn: "(v) -> body" — inline body with recv.
                    // Function ref fn: emit as fn.apply(recv).
                    String fnApplied;
                    if (fn.contains("->")) {
                        // Best-effort lambda inlining: replace single-param
                        // with receiver.
                        java.util.regex.Matcher m = java.util.regex.Pattern.compile(
                            "^\\s*\\(?\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*\\)?\\s*->\\s*(.+)$"
                        ).matcher(fn);
                        if (m.find()) {
                            fnApplied = replaceIdentifier(m.group(2).trim(), m.group(1), receiver.get().text());
                        } else {
                            fnApplied = "(" + fn + ").apply(" + receiver.get().text() + ")";
                        }
                    } else if ("str.to_string".equals(fn) || "String.to_string".equals(fn)
                            || fn.endsWith(".to_string")) {
                        // Method ref to identity-style String conversion.
                        fnApplied = "String.valueOf(" + receiver.get().text() + ")";
                    } else {
                        fnApplied = "((java.util.function.Function)(" + fn + ")).apply(" + receiver.get().text() + ")";
                    }
                    return Optional.of(new ShapeExpression(
                            "(" + receiver.get().text() + " != null ? " + fnApplied + " : null)",
                            ""));
                }
                // .map(f) on Option: same semantics, same translation.
                if ("map".equals(methodName) && callArgs.size() == 1) {
                    String fn = callArgs.get(0);
                    String fnApplied = applyFnToReceiver(fn, receiver.get().text());
                    return Optional.of(new ShapeExpression(
                            "(" + receiver.get().text() + " != null ? " + fnApplied + " : null)",
                            ""));
                }
                if ("unwrap_or_else".equals(methodName) && callArgs.size() == 1) {
                    // .unwrap_or_else(|e| f(e)) — inline the lambda body
                    // with the Err value (best-effort: pass null since
                    // post-erasure we don't have a typed Err carrier).
                    String fn = callArgs.get(0);
                    String fnInvoked;
                    if (fn.contains("->")) {
                        java.util.regex.Matcher m = java.util.regex.Pattern.compile(
                            "^\\s*\\(?\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*\\)?\\s*->\\s*(.+)$"
                        ).matcher(fn);
                        if (m.find()) {
                            fnInvoked = replaceIdentifier(m.group(2).trim(), m.group(1), "null");
                        } else {
                            fnInvoked = "((java.util.function.Function)(" + fn + ")).apply(null)";
                        }
                    } else {
                        fnInvoked = "((java.util.function.Function)(" + fn + ")).apply(null)";
                    }
                    return Optional.of(new ShapeExpression(
                            "(" + receiver.get().text() + " != null ? " + receiver.get().text() + " : " + fnInvoked + ")",
                            ""));
                }
                // .is_none() → == null; .is_some() → != null.
                if ("is_none".equals(methodName)) {
                    return Optional.of(new ShapeExpression(
                            "(" + receiver.get().text() + " == null)", "boolean"));
                }
                if ("is_some".equals(methodName)) {
                    return Optional.of(new ShapeExpression(
                            "(" + receiver.get().text() + " != null)", "boolean"));
                }
                return Optional.of(new ShapeExpression(
                        receiver.get().text() + "." + javaMethod + "(" + javaArgs + ")",
                        ""));  // unknown call return; let var infer
            }
        }
        // concept:ref(inner, mutability_leaf) — the mutability arg is a leaf
        // without operand bindings; lower only the inner.
        if (conceptMatches("concept:ref", conceptName) && args.size() >= 1) {
            Optional<ShapeExpression> inner = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            return inner;
        }
        List<ShapeExpression> argTerms = new ArrayList<>();
        for (int i = 0; i < args.size(); i++) {
            Optional<ShapeExpression> term = lowerShapeExpression(args.get(i), context, appendPosition(position, i));
            if (term.isEmpty() || term.get().text().isBlank()) {
                return Optional.empty();
            }
            argTerms.add(term.get());
        }
        if (conceptMatches("concept:call", conceptName) && !argTerms.isEmpty()) {
            // walk_rpc emits method calls AS concept:call with a method-leaf
            // at args[1] (kind:"method", text:"<name>") and receiver at args[0].
            // Plain calls have a path/symbol callee at args[0]. Distinguish
            // by inspecting the raw shape, not the lowered ShapeExpression.
            if (args.size() >= 2) {
                Jcs.Json secondArg = args.get(1);
                if (secondArg instanceof Jcs.Obj methodMaybe) {
                    String kind = methodMaybe.stringFieldOrNull("kind");
                    if ("method".equals(kind)) {
                        String methodName = methodMaybe.stringFieldOrNull("text");
                        if (methodName != null && !methodName.isBlank()) {
                            String receiver = argTerms.get(0).text();
                            String joined = argTerms.stream().skip(2)
                                    .map(ShapeExpression::text)
                                    .collect(Collectors.joining(", "));
                            return Optional.of(new ShapeExpression(
                                    receiver + "." + methodName + "(" + joined + ")",
                                    mapSourceType(context.returnType)));
                        }
                    }
                }
            }
            String callee = argTerms.get(0).text();
            // Synthetic tuple constructor from the lifter: emit as Object[].
            if ("__provekit_tuple_new".equals(callee)) {
                String joined = argTerms.stream().skip(1).map(ShapeExpression::text).collect(Collectors.joining(", "));
                return Optional.of(new ShapeExpression("new Object[] {" + joined + "}", "Object[]"));
            }
            // Rust enum variant constructors with no path qualifier:
            // - `Ok(x)` returns the wrapped value (java has no Result).
            // - `Err(x)` throws RuntimeException with the value's string form.
            // - `Some(x)` returns the value (java null = None).
            // - `None` is null (handled via leaf).
            if ("Ok".equals(callee) || "Some".equals(callee)) {
                String inner = argTerms.size() > 1 ? argTerms.get(1).text() : "null";
                return Optional.of(new ShapeExpression(inner, argTerms.size() > 1 ? argTerms.get(1).typeName() : ""));
            }
            if ("Err".equals(callee)) {
                String inner = argTerms.size() > 1 ? argTerms.get(1).text() : "\"err\"";
                context.recordApproximation(
                    "result-err-as-throw",
                    "Rust Err(" + inner + ") lowered as throw RuntimeException. "
                        + "Variant identity and structured error payload are lost.");
                return Optional.of(new ShapeExpression(
                        "throw new RuntimeException(String.valueOf(" + inner + "))",
                        ""));
            }
            // `str::to_string` method ref → identity (already a String).
            if ("str.to_string".equals(callee) || "String.from".equals(callee)) {
                String inner = argTerms.size() > 1 ? argTerms.get(1).text() : "\"\"";
                return Optional.of(new ShapeExpression(inner, "String"));
            }
            // Value::String(s) (or its `::` → `.` form JsonNode.String) →
            // build a Jackson TextNode via MAPPER.valueToTree. Same for
            // Value::Number, Value::Bool — all collapse to valueToTree
            // since Jackson coerces the primitive to the right node type.
            if (("Value.String".equals(callee) || "Value.Number".equals(callee) || "Value.Bool".equals(callee)
                    || callee.endsWith("JsonNode.String") || callee.endsWith("JsonNode.Number")
                    || callee.endsWith("JsonNode.Bool"))
                    && argTerms.size() == 2) {
                return Optional.of(new ShapeExpression(
                        "MAPPER.valueToTree(" + argTerms.get(1).text() + ")",
                        "com.fasterxml.jackson.databind.JsonNode"));
            }
            // Generic enum variant constructor with a path:
            // `LiftError::Internal(msg)`, `Status::Failed(reason)`, etc.
            // No enum infrastructure in java by default — reduce to the
            // payload value as a string. Detect via capitalized identifier
            // segments + uppercase variant.
            if (callee.contains(".") && argTerms.size() == 2) {
                String[] parts = callee.split("\\.");
                if (parts.length == 2
                        && parts[0].length() > 0 && Character.isUpperCase(parts[0].charAt(0))
                        && parts[1].length() > 0 && Character.isUpperCase(parts[1].charAt(0))) {
                    context.recordApproximation(
                        "enum-variant-flattened-to-string",
                        "Rust " + parts[0] + "::" + parts[1] + "(" + argTerms.get(1).text()
                            + ") lowered as String.valueOf(payload). Variant identity erased.");
                    return Optional.of(new ShapeExpression(
                            "String.valueOf(" + argTerms.get(1).text() + ")",
                            "String"));
                }
            }
            // Rust path expressions use `::` (e.g. String::new, Vec::with_capacity).
            // For java, translate to `.` (String.new isn't valid — translate to
            // common constructor pattern when the suffix is `new`).
            // Type-name remaps for rust std → java std:
            callee = callee
                .replace("Vec::", "java.util.ArrayList::")
                .replace("BTreeSet::", "java.util.TreeSet::")
                .replace("BTreeMap::", "java.util.TreeMap::")
                .replace("HashSet::", "java.util.HashSet::")
                .replace("HashMap::", "java.util.HashMap::")
                .replace("PathBuf::from", "java.nio.file.Path::of")
                .replace("PathBuf::", "java.nio.file.Path::");
            String calleeJava = callee.replace("::", ".");
            if (calleeJava.endsWith(".new")) {
                // String::new() → new String(), Vec::new() → new ArrayList<>(), etc.
                String typeName = calleeJava.substring(0, calleeJava.length() - 4);
                // Generic types need a <> for diamond inference.
                boolean needsDiamond = typeName.contains("ArrayList") || typeName.contains("HashMap")
                        || typeName.contains("HashSet") || typeName.contains("TreeMap")
                        || typeName.contains("TreeSet") || typeName.contains("LinkedList");
                String diamond = needsDiamond ? "<>" : "";
                String joined = argTerms.stream().skip(1).map(ShapeExpression::text).collect(Collectors.joining(", "));
                return Optional.of(new ShapeExpression("new " + typeName + diamond + "(" + joined + ")", typeName));
            }
            // `Path::from(p)` — when arg is already a Path, just use it.
            // Otherwise convert to string and use Path.of.
            if ("java.nio.file.Path.from".equals(calleeJava) || "Path.from".equals(calleeJava)) {
                String joined = argTerms.stream().skip(1).map(ShapeExpression::text).collect(Collectors.joining(", "));
                return Optional.of(new ShapeExpression(
                        "java.nio.file.Path.of(" + joined + ".toString())",
                        "java.nio.file.Path"));
            }
            // `String::with_capacity(n)` is rust idiom. Java has no direct
            // equivalent — Strings are immutable; the rust code uses
            // String here to build via push_str/push, which is really a
            // StringBuilder pattern in java. Emit `new StringBuilder(n)`.
            if ("String.with_capacity".equals(calleeJava)) {
                String joined = argTerms.stream().skip(1).map(ShapeExpression::text).collect(Collectors.joining(", "));
                return Optional.of(new ShapeExpression("new StringBuilder(" + joined + ")", "StringBuilder"));
            }
            if (calleeJava.endsWith(".with_capacity")) {
                // Vec::with_capacity(n) → new ArrayList<>(n), generic.
                String typeName = calleeJava.substring(0, calleeJava.length() - ".with_capacity".length());
                if ("Vec".equals(typeName)) typeName = "java.util.ArrayList<>";
                String joined = argTerms.stream().skip(1).map(ShapeExpression::text).collect(Collectors.joining(", "));
                return Optional.of(new ShapeExpression("new " + typeName + "(" + joined + ")", typeName));
            }
            // Allow identifiers + dotted paths (after :: → . translation).
            if (!isIdentifier(calleeJava) && !calleeJava.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)+")) {
                return Optional.empty();
            }
            String joined = argTerms.stream().skip(1).map(ShapeExpression::text).collect(Collectors.joining(", "));
            // Function-return-type catalog: look up raw rust type if known,
            // map to java syntax for the expression's typeName.
            String returnType = context.lookupReturnType(callee);
            String javaReturnType = returnType.isBlank() ? "" : mapSourceType(returnType);
            return Optional.of(new ShapeExpression(calleeJava + "(" + joined + ")", javaReturnType));
        }
        if (conceptMatches("concept:field", conceptName) && args.size() == 2) {
            // args[0]: receiver expression. args[1]: field name leaf.
            String fieldName = args.get(1).stringFieldOrNull("text");
            if (fieldName == null || fieldName.isBlank()) return Optional.empty();
            String receiver = argTerms.get(0).text();
            return Optional.of(new ShapeExpression(
                    receiver + "." + fieldName,
                    mapSourceType(context.returnType)));
        }
        if (conceptMatches("concept:index", conceptName) && argTerms.size() == 2) {
            // Java: array[idx] OR list.get(idx). Default to array form;
            // collections use method-call concept instead.
            String receiver = argTerms.get(0).text();
            String idx = argTerms.get(1).text();
            return Optional.of(new ShapeExpression(
                    receiver + "[" + idx + "]",
                    mapSourceType(context.returnType)));
        }
        if (conceptMatches("concept:cast", conceptName) && args.size() == 2) {
            // args[0]: value. args[1]: type leaf.
            String typeName = args.get(1).stringFieldOrNull("text");
            if (typeName == null || typeName.isBlank()) {
                Optional<ShapeExpression> t = lowerShapeExpression(args.get(1), context, appendPosition(position, 1));
                if (t.isEmpty()) return Optional.empty();
                typeName = t.get().text();
            }
            // Array indexing in java requires int. `as usize` is rust's
            // platform-width unsigned; for typical use (indices, sizes),
            // mapping to int is correct in java (idx must be int).
            // `as char` on a byte/int returns char; java cast (char) works.
            String javaType = switch (typeName.trim()) {
                case "usize", "isize" -> "int";  // array indices want int
                case "char" -> "char";
                default -> mapSourceType(typeName);
            };
            String value = argTerms.get(0).text();
            return Optional.of(new ShapeExpression(
                    "(" + javaType + ") (" + value + ")",
                    javaType));
        }
        if (conceptMatches("concept:closure", conceptName) && !argTerms.isEmpty()) {
            // Rust walk_rpc emits concept:closure with args=[body, param1,
            // param2, ...]. Java lambda syntax is (a, b) -> body. Capture
            // semantics are simplified to lexical capture (java closes over
            // effectively-final locals).
            String body = argTerms.get(0).text();
            String params = argTerms.subList(1, argTerms.size()).stream()
                    .map(ShapeExpression::text)
                    .collect(Collectors.joining(", "));
            return Optional.of(new ShapeExpression(
                    "(" + params + ") -> " + body,
                    mapSourceType(context.returnType)));
        }
        // concept:reference / concept:ref (rust &x or &mut x) + concept:deref (*x):
        // java has neither explicit reference nor deref operators. References
        // are implicit on objects; deref is a no-op. Pass the inner expression
        // through. The substrate emits concept:ref with TWO args (inner +
        // mutability_leaf); the older alias concept:reference has one arg.
        if (conceptMatches("concept:ref", conceptName) && argTerms.size() >= 1) {
            return Optional.of(argTerms.get(0));
        }
        if ((conceptMatches("concept:reference", conceptName) || conceptMatches("concept:deref", conceptName))
                && argTerms.size() == 1) {
            return Optional.of(argTerms.get(0));
        }
        String expression = operationExpression(conceptName, argTerms);
        if (expression == null) {
            return Optional.empty();
        }
        return Optional.of(new ShapeExpression(expression, operationReturnType(conceptName, argTerms, context.returnType)));
    }

    private static ShapeExpression shapeLeafExpression(
            Jcs.Obj shape,
            ShapeContext context,
            List<Integer> position) {
        String bound = context.operandBindings.get(position);
        if (bound != null && !bound.isBlank()) {
            return symbolTerm(bound, context);
        }
        String kind = shape.stringFieldOrNull("kind");
        if ("var".equals(kind)) {
            String name = shape.stringFieldOrNull("name");
            if (name != null && !name.isBlank()) {
                return new ShapeExpression(name, typeForArgument(name, context));
            }
        }
        // Rust lifter emits {kind: "symbol", text: "<ident>"} for variable
        // references and {kind: "path", text: "<path>"} for fn/type paths.
        // Both are valid java identifiers (after :: → . translation).
        if ("symbol".equals(kind) || "path".equals(kind)) {
            String text = shape.stringFieldOrNull("text");
            if (text != null && !text.isBlank()) {
                // Rust → java identifier remaps:
                // - `Value::Null` (serde_json) → MAPPER.nullNode() (Jackson)
                // - `null` (placeholder) → null
                // - rust std type names → java std equivalents
                String remapped = switch (text) {
                    case "Value::Null", "Value.Null" -> "MAPPER.nullNode()";
                    case "Value::as_str" -> "(java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, String>) com.fasterxml.jackson.databind.JsonNode::asText";
                    case "Value::as_array", "Value.as_array" ->
                        "(java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, com.fasterxml.jackson.databind.JsonNode>) (n -> n != null && n.isArray() ? n : null)";
                    case "Value::as_object", "Value.as_object" ->
                        "(java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, com.fasterxml.jackson.databind.JsonNode>) (n -> n != null && n.isObject() ? n : null)";
                    case "null" -> "null";
                    // Bare `str` / `String::to_string` references — convert
                    // method references to lambdas where java reference syntax
                    // can't apply.
                    default -> {
                        if (text.startsWith("str.")) yield text.replace("str.", "");
                        yield text
                                .replace("Value::Null", "MAPPER.nullNode()")
                                .replace("Value::as_str", "com.fasterxml.jackson.databind.JsonNode::asText")
                                .replace("Value::", "com.fasterxml.jackson.databind.JsonNode::")
                                .replace("Vec::", "java.util.ArrayList::")
                                .replace("BTreeSet::", "java.util.TreeSet::")
                                .replace("BTreeMap::", "java.util.TreeMap::")
                                .replace("HashSet::", "java.util.HashSet::")
                                .replace("HashMap::", "java.util.HashMap::")
                                .replace("PathBuf::", "java.nio.file.Path::")
                                .replace("::", ".");
                    }
                };
                return new ShapeExpression(remapped, typeForArgument(remapped, context));
            }
        }
        Jcs.Json value = shape.get("value");
        if ("const".equals(kind) || value != null) {
            return literalTerm(value);
        }
        return context.fallbackLeaf();
    }

    private static String operationExpression(String conceptName, List<ShapeExpression> args) {
        String op = conceptName.startsWith("concept:") ? conceptName.substring("concept:".length()) : conceptName;
        if (args.size() == 2) {
            String left = args.get(0).text();
            String right = args.get(1).text();
            return switch (op) {
                case "add" -> "(" + left + ") + (" + right + ")";
                case "sub" -> "(" + left + ") - (" + right + ")";
                case "mul" -> "(" + left + ") * (" + right + ")";
                case "div" -> "(" + left + ") / (" + right + ")";
                case "mod" -> "(" + left + ") % (" + right + ")";
                case "eq" -> {
                    // String == in rust is value equality; java needs .equals.
                    // If either side is a string literal, use Objects.equals
                    // (handles null + values uniformly).
                    if (isStringLiteral(left) || isStringLiteral(right)) {
                        yield "java.util.Objects.equals(" + left + ", " + right + ")";
                    }
                    yield "(" + left + ") == (" + right + ")";
                }
                case "ne" -> {
                    if (isStringLiteral(left) || isStringLiteral(right)) {
                        yield "!java.util.Objects.equals(" + left + ", " + right + ")";
                    }
                    yield "(" + left + ") != (" + right + ")";
                }
                case "lt" -> "(" + left + ") < (" + right + ")";
                case "le" -> "(" + left + ") <= (" + right + ")";
                case "gt" -> "(" + left + ") > (" + right + ")";
                case "ge" -> "(" + left + ") >= (" + right + ")";
                case "and" -> "(" + left + ") && (" + right + ")";
                case "or" -> "(" + left + ") || (" + right + ")";
                case "bitand" -> "(" + left + ") & (" + right + ")";
                case "bitor" -> "(" + left + ") | (" + right + ")";
                case "bitxor" -> "(" + left + ") ^ (" + right + ")";
                case "shl" -> "(" + left + ") << (" + right + ")";
                case "shr" -> "(" + left + ") >> (" + right + ")";
                default -> null;
            };
        }
        if (args.size() == 1) {
            String value = args.get(0).text();
            return switch (op) {
                case "neg" -> "-(" + value + ")";
                case "not" -> "!(" + value + ")";
                case "bitnot" -> "~(" + value + ")";
                default -> null;
            };
        }
        if ("skip".equals(op) && args.isEmpty()) {
            return "null";
        }
        return null;
    }

    private static String operationReturnType(
            String conceptName,
            List<ShapeExpression> args,
            String fallbackReturnType) {
        String op = conceptName.startsWith("concept:") ? conceptName.substring("concept:".length()) : conceptName;
        return switch (op) {
            case "eq", "ne", "lt", "le", "gt", "ge", "and", "or", "not" -> "boolean";
            // Call return type is genuinely unknown without a type DB.
            // Returning blank lets the caller use `var` (java type
            // inference) instead of guessing the enclosing function's
            // return type.
            case "call" -> "";
            default -> args.isEmpty() ? "" : args.get(0).typeName();
        };
    }

    /**
     * Lower rust's `json!({...})` macro to Jackson ObjectNode construction.
     * The body is raw rust source text like `{ "k" : "v", "id" : id }`.
     * For each key:value pair, emit `.put(...)` or `.set(...)` calls on a
     * fresh ObjectNode. String/number/bool literals → .put(); identifiers
     * and other expressions → .set() (treated as JsonNode).
     *
     * Returns null if the body can't be parsed.
     */
    private static int jsonMacroDepth = 0;

    private static String emitJsonMacro(String body, ShapeContext context) {
        // Unique var name per nested lambda — java forbids shadowing outer
        // local vars in nested lambdas.
        int depth = ++jsonMacroDepth;
        try {
            return emitJsonMacroInner(body, context, depth);
        } finally {
            jsonMacroDepth--;
        }
    }

    private static String emitJsonMacroInner(String body, ShapeContext context, int depth) {
        String objName = "__obj" + depth;
        String arrName = "__arr" + depth;
        String trimmed = body.trim();
        if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
            String inside = trimmed.substring(1, trimmed.length() - 1).trim();
            List<String> parts = splitTopLevelCommas(inside);
            // Use a Supplier-wrapped builder: ObjectNode.set returns JsonNode
            // in jackson, breaking fluent chains. The Supplier form keeps
            // each call on the local ObjectNode variable.
            StringBuilder sb = new StringBuilder();
            sb.append("((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { ");
            sb.append("com.fasterxml.jackson.databind.node.ObjectNode ").append(objName).append(" = MAPPER.createObjectNode(); ");
            for (String part : parts) {
                if (part.isEmpty()) continue;
                int colon = findTopLevelColon(part);
                if (colon < 0) return null;
                String keyText = part.substring(0, colon).trim();
                String valueText = part.substring(colon + 1).trim();
                String key = stripQuotes(keyText);
                if (key == null) return null;
                if (valueText.startsWith("{") || valueText.startsWith("[")) {
                    String nested = emitJsonMacro(valueText, context);
                    if (nested == null) return null;
                    sb.append("$OBJ$.set(").append(quote(key)).append(", ").append(nested).append("); ");
                } else if (valueText.startsWith("\"") && valueText.endsWith("\"")) {
                    sb.append("$OBJ$.put(").append(quote(key)).append(", ").append(valueText).append("); ");
                } else if (valueText.equals("true") || valueText.equals("false")) {
                    sb.append("$OBJ$.put(").append(quote(key)).append(", ").append(valueText).append("); ");
                } else if (valueText.matches("-?\\d+(\\.\\d+)?")) {
                    sb.append("$OBJ$.put(").append(quote(key)).append(", ").append(valueText).append("); ");
                } else {
                    String paramType = typeForArgument(valueText, context);
                    boolean isPrim = paramType != null && (
                            "long".equals(paramType) || "int".equals(paramType) ||
                            "double".equals(paramType) || "float".equals(paramType) ||
                            "boolean".equals(paramType) || "String".equals(paramType));
                    boolean isMethodCall = valueText.contains("(") && valueText.contains(")");
                    if (isPrim) {
                        sb.append("$OBJ$.put(").append(quote(key)).append(", ").append(valueText).append("); ");
                    } else if ("com.fasterxml.jackson.databind.JsonNode".equals(paramType)) {
                        sb.append("$OBJ$.set(").append(quote(key)).append(", ").append(valueText).append("); ");
                    } else if (isMethodCall) {
                        // Method-call result of unknown type — wrap via
                        // MAPPER.valueToTree which accepts String/long/
                        // double/boolean/JsonNode uniformly.
                        sb.append("$OBJ$.set(").append(quote(key)).append(", MAPPER.valueToTree(")
                          .append(valueText).append(")); ");
                    } else {
                        // Plain identifier of unknown type — best-effort .set;
                        // valueToTree wrap if needed at call site.
                        sb.append("$OBJ$.set(").append(quote(key)).append(", MAPPER.valueToTree(")
                          .append(valueText).append(")); ");
                    }
                }
            }
            sb.append("return ").append(objName).append("; }).get()");
            return sb.toString().replace("$OBJ$", objName);
        }
        if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
            String inside = trimmed.substring(1, trimmed.length() - 1).trim();
            List<String> parts = splitTopLevelCommas(inside);
            StringBuilder sb = new StringBuilder();
            sb.append("((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { ");
            sb.append("com.fasterxml.jackson.databind.node.ArrayNode ").append(arrName).append(" = MAPPER.createArrayNode(); ");
            for (String part : parts) {
                String v = part.trim();
                if (v.isEmpty()) continue;
                if (v.startsWith("{") || v.startsWith("[")) {
                    String nested = emitJsonMacro(v, context);
                    if (nested == null) return null;
                    sb.append(arrName).append(".add(").append(nested).append("); ");
                } else {
                    sb.append(arrName).append(".add(").append(v).append("); ");
                }
            }
            sb.append("return ").append(arrName).append("; }).get()");
            return sb.toString();
        }
        return null;
    }

    /**
     * Lower rust's `format!(...)` macro. Body looks like:
     *   `"text {} text", expr1, expr2`
     * Translate to java String.format with %s placeholders.
     */
    private static String emitFormatMacro(String body, ShapeContext context) {
        List<String> parts = splitTopLevelCommas(body.trim());
        if (parts.isEmpty()) return null;
        String fmt = parts.get(0).trim();
        if (!fmt.startsWith("\"") || !fmt.endsWith("\"")) return null;
        // Replace rust {} placeholders with java %s. {:?} debug uses
        // String.valueOf in java since there's no built-in debug format.
        String javaFmt = fmt.replace("{}", "%s").replace("{:?}", "%s");
        StringBuilder sb = new StringBuilder();
        sb.append("String.format(").append(javaFmt);
        for (int i = 1; i < parts.size(); i++) {
            sb.append(", ").append(parts.get(i).trim());
        }
        sb.append(")");
        return sb.toString();
    }

    /** Split a string on commas at top nesting level (depth 0 of {} [] () "")  */
    private static List<String> splitTopLevelCommas(String s) {
        List<String> out = new ArrayList<>();
        int depth = 0;
        boolean inString = false;
        int start = 0;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (inString) {
                if (c == '\\' && i + 1 < s.length()) { i++; continue; }
                if (c == '"') inString = false;
                continue;
            }
            switch (c) {
                case '"' -> inString = true;
                case '{', '[', '(' -> depth++;
                case '}', ']', ')' -> depth--;
                case ',' -> {
                    if (depth == 0) {
                        out.add(s.substring(start, i));
                        start = i + 1;
                    }
                }
                default -> {}
            }
        }
        if (start < s.length()) out.add(s.substring(start));
        return out;
    }

    /** Find the first top-level colon (not inside quotes/brackets). */
    private static int findTopLevelColon(String s) {
        int depth = 0;
        boolean inString = false;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (inString) {
                if (c == '\\' && i + 1 < s.length()) { i++; continue; }
                if (c == '"') inString = false;
                continue;
            }
            switch (c) {
                case '"' -> inString = true;
                case '{', '[', '(' -> depth++;
                case '}', ']', ')' -> depth--;
                case ':' -> { if (depth == 0) return i; }
                default -> {}
            }
        }
        return -1;
    }

    /** Strip surrounding quotes from a string literal. Returns the inner
     *  text or null if not a valid quoted string. */
    private static String stripQuotes(String s) {
        String t = s.trim();
        if (t.length() >= 2 && t.startsWith("\"") && t.endsWith("\"")) {
            return t.substring(1, t.length() - 1);
        }
        return null;
    }

    private static String quote(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\t' -> sb.append("\\t");
                default -> sb.append(c);
            }
        }
        sb.append('"');
        return sb.toString();
    }

    /** True iff the outer `(` matches the outer `)` (i.e. no unbalanced
     *  closing inside that would split the expression). */
    private static boolean matchingOuterParens(String s) {
        if (s.length() < 2 || s.charAt(0) != '(' || s.charAt(s.length() - 1) != ')') return false;
        int depth = 0;
        boolean inString = false;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (inString) {
                if (c == '\\' && i + 1 < s.length()) { i++; continue; }
                if (c == '"') inString = false;
                continue;
            }
            if (c == '"') { inString = true; continue; }
            if (c == '(') depth++;
            else if (c == ')') {
                depth--;
                if (depth == 0 && i < s.length() - 1) return false;
            }
        }
        return depth == 0;
    }

    /** True iff every non-wildcard match arm has a string-literal pattern. */
    private static boolean allPatternsStringLiteral(List<Jcs.Obj> matchArgs) {
        boolean anyStringPattern = false;
        for (int i = 1; i < matchArgs.size(); i++) {
            Jcs.Obj arm = matchArgs.get(i);
            if (!"concept:match-arm".equals(shapeConceptName(arm))) continue;
            List<Jcs.Obj> armArgs = shapeArgs(arm);
            if (armArgs.size() < 2) continue;
            String pat = armArgs.get(0).stringFieldOrNull("text");
            if (pat == null) return false;
            pat = pat.trim();
            if ("_".equals(pat)) continue;  // wildcard ok
            if (isStringLiteral(pat)) { anyStringPattern = true; continue; }
            return false;
        }
        return anyStringPattern;
    }

    /** Extract a tuple's element type at index `idx` from a rust tuple type
     *  string like `(JsonNode, boolean)`. Returns null if can't parse. */
    private static String tupleElementType(String tupleType, int idx) {
        String t = tupleType.trim();
        if (!t.startsWith("(") || !t.endsWith(")")) return null;
        String inner = t.substring(1, t.length() - 1).trim();
        // Split top-level commas.
        java.util.List<String> elems = new java.util.ArrayList<>();
        int depth = 0;
        int start = 0;
        for (int i = 0; i < inner.length(); i++) {
            char c = inner.charAt(i);
            if (c == '<' || c == '(' || c == '[') depth++;
            else if (c == '>' || c == ')' || c == ']') depth--;
            else if (c == ',' && depth == 0) {
                elems.add(inner.substring(start, i).trim());
                start = i + 1;
            }
        }
        if (start < inner.length()) elems.add(inner.substring(start).trim());
        if (idx < 0 || idx >= elems.size()) return null;
        return elems.get(idx);
    }

    /** Extract the called function name from a text like `handle_line(line, adapter)`.
     *  Returns null if not a simple call. */
    private static String extractCalledFnName(String text) {
        if (text == null) return null;
        int paren = text.indexOf('(');
        if (paren <= 0) return null;
        String head = text.substring(0, paren).trim();
        if (head.matches("[A-Za-z_][A-Za-z0-9_]*")) return head;
        return null;
    }

    /** Apply a function-like expression `fn` to a single argument expression.
     *  Handles three forms: lambda (inlined by substitution), known str-conv
     *  refs (identity-like), generic method refs (Function cast + apply). */
    private static String applyFnToReceiver(String fn, String arg) {
        if (fn == null) return arg;
        String t = fn.trim();
        if (t.contains("->")) {
            java.util.regex.Matcher m = java.util.regex.Pattern.compile(
                "^\\s*\\(?\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*\\)?\\s*->\\s*(.+)$"
            ).matcher(t);
            if (m.find()) {
                return replaceIdentifier(m.group(2).trim(), m.group(1), arg);
            }
        }
        if ("str.to_string".equals(t) || "String.to_string".equals(t)
                || t.endsWith(".to_string") || t.equals("String.valueOf")) {
            return "String.valueOf(" + arg + ")";
        }
        // Method ref (Type::method or instance::method) — apply via Function.
        return "((java.util.function.Function)(" + t + ")).apply(" + arg + ")";
    }

    /** True iff text looks like a java string literal (starts + ends with "). */
    private static boolean isStringLiteral(String s) {
        if (s == null) return false;
        String t = s.trim();
        return t.length() >= 2 && t.startsWith("\"") && t.endsWith("\"");
    }

    /** Lower a concept:match in statement-position assigning each arm's
     *  body to `targetName`. Arms whose bodies are control-flow
     *  statements (return/break/continue) are emitted as such. Returns
     *  the full block (declaration of target + if-else chain) or null
     *  if the shape can't be lowered. */
    private static String lowerMatchAsAssignmentTo(
            String targetName, Jcs.Obj matchShape, ShapeContext context, List<Integer> position) {
        List<Jcs.Obj> args = shapeArgs(matchShape);
        if (args.size() < 2) return null;
        Optional<ShapeExpression> scrut = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
        if (scrut.isEmpty()) return null;
        String scrutVar = context.tempName();
        context.definedSymbols.add(scrutVar);
        StringBuilder out = new StringBuilder();
        out.append("var ").append(scrutVar).append(" = ").append(scrut.get().text()).append(";\n");
        // Declare target with scrut's inferred type if available; else
        // JsonNode (most common Ok-arm case for json_parse / Value::Object).
        String targetType = scrut.get().typeName();
        if (targetType == null || targetType.isBlank() || "Object".equals(targetType)) {
            targetType = "com.fasterxml.jackson.databind.JsonNode";
        }
        out.append(targetType).append(" ").append(targetName).append(";\n");
        boolean defaultSeen = false;
        int armIdx = 0;
        for (int i = 1; i < args.size(); i++) {
            Jcs.Obj arm = args.get(i);
            if (!conceptMatches("concept:match-arm", shapeConceptName(arm))) continue;
            List<Jcs.Obj> armArgs = shapeArgs(arm);
            if (armArgs.size() < 2) continue;
            String patternText = armArgs.get(0).stringFieldOrNull("text");
            if (patternText == null) patternText = "";
            patternText = patternText.trim();
            String boundVar = bindingFromPattern(patternText);
            if (boundVar != null) context.definedSymbols.add(boundVar);
            String cond = patternToCondition(patternText, scrutVar);
            // Try body as a STATEMENT (control-flow). Fall back to expression.
            Optional<String> bodyStmt = lowerShapeBody(armArgs.get(1), context, appendPosition(position, i));
            String bodyEmit;
            if (bodyStmt.isPresent()) {
                bodyEmit = bodyStmt.get();
                if (boundVar != null) bodyEmit = replaceIdentifier(bodyEmit, boundVar, scrutVar);
            } else {
                Optional<ShapeExpression> bodyExpr = lowerShapeExpression(armArgs.get(1), context, appendPosition(position, i));
                if (bodyExpr.isEmpty()) return null;
                String bodyText = bodyExpr.get().text();
                if (boundVar != null) {
                    String sub = patternHasNestedBinding(patternText)
                            ? "String.valueOf(" + scrutVar + ")"
                            : scrutVar;
                    bodyText = replaceIdentifier(bodyText, boundVar, sub);
                }
                bodyEmit = targetName + " = " + bodyText + ";";
            }
            if ("true".equals(cond)) {
                if (armIdx == 0) {
                    out.append(bodyEmit);
                } else {
                    out.append(" else {\n    ").append(bodyEmit).append("\n}");
                }
                defaultSeen = true;
                break;
            }
            if (armIdx == 0) out.append("if (").append(cond).append(") {\n");
            else out.append(" else if (").append(cond).append(") {\n");
            out.append("    ").append(bodyEmit).append("\n}");
            armIdx++;
        }
        if (!defaultSeen && armIdx > 0) {
            // Rust source was exhaustive (no wildcard) — but java's
            // ternary chain needs an else. Substrate-honest: emit a
            // panic + loss_record entry, rather than synthesizing a
            // null default the source didn't authorize. If the rust
            // match was truly exhaustive, this branch is unreachable;
            // if it ISN'T (substrate canonicalized a non-exhaustive
            // pattern), the panic surfaces the gap at runtime instead
            // of silently returning null.
            context.recordApproximation(
                "match-exhaustive-synthesized-default",
                "Rust match for assignment to '" + targetName + "' had no wildcard. "
                    + "Java requires an else branch; emitted a panic since the source "
                    + "expressed exhaustiveness. If callers reach the else, the substrate "
                    + "missed an arm.");
            out.append(" else { throw new RuntimeException(\"exhaustive match without arm: ")
               .append(targetName).append("\"); }");
        }
        return out.toString();
    }

    /** Map common rust method names to java equivalents. Falls through to identity. */
    private static String mapRustMethodToJava(String rustMethod) {
        return switch (rustMethod) {
            case "to_string" -> "toString";
            case "to_owned" -> "toString";
            case "len" -> "length";
            case "push_str" -> "append";
            case "push" -> "append";
            case "is_empty" -> "isEmpty";
            case "starts_with" -> "startsWith";
            case "ends_with" -> "endsWith";
            case "contains" -> "contains";
            case "is_null" -> "isNull";
            case "as_str" -> "asText";
            case "as_array" -> "elements";
            case "field_names" -> "fieldNames";
            // .cloned() on Option<&T> in rust is null-safe (None → None,
            // Some(&t) → Some(t.clone())). In our java erasure where
            // Option<T> = T-or-null and references are already shared,
            // .cloned() is a no-op. Mapping to .deepCopy() throws NPE
            // on null receivers; treat as identity instead.
            case "cloned" -> "_provekit_identity";
            default -> rustMethod;
        };
    }

    /** True iff the pattern contains a nested enum-variant binding like
     *  `Err(LiftError::Internal(msg))`. Nested binds typically destructure
     *  a String-typed enum variant payload, so the substitution wraps the
     *  scrutinee in String.valueOf for safety. */
    private static boolean patternHasNestedBinding(String pattern) {
        String t = pattern.trim();
        int ifIdx = t.indexOf(" if ");
        if (ifIdx > 0) t = t.substring(0, ifIdx).trim();
        return t.matches("^(?:Some|Ok|Err)\\s*\\(\\s*[A-Z].*\\(\\s*[A-Za-z_][A-Za-z0-9_]*\\s*\\)\\s*\\)$")
            || java.util.regex.Pattern.compile("::").matcher(t).find();
    }

    /** For pattern text like `Some(v)` or `Ok(x)` or bare identifier `other`
     *  or nested `Err(Type::Variant(msg))`, return the bound var. */
    private static String bindingFromPattern(String pattern) {
        String t = pattern.trim();
        // Strip guard before checking.
        int ifIdx = t.indexOf(" if ");
        if (ifIdx > 0) t = t.substring(0, ifIdx).trim();
        // Outer Variant(name) form.
        java.util.regex.Matcher m = java.util.regex.Pattern.compile(
            "^(?:Some|Ok|Err|None)\\s*\\(\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*\\)$"
        ).matcher(t);
        if (m.find()) return m.group(1);
        // Nested: `Err(Type::Variant(x))` or `Some(Inner(x))` — pull
        // the innermost ident.
        java.util.regex.Matcher nested = java.util.regex.Pattern.compile(
            "\\(\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*\\)\\s*\\)\\s*$"
        ).matcher(t);
        if (nested.find()) return nested.group(1);
        // Bare identifier (rust catch-all binding like `other => ...`).
        if (t.matches("^[A-Za-z_][A-Za-z0-9_]*$") && !t.equals("_") && !t.equals("None")) {
            return t;
        }
        return null;
    }

    /**
     * Translate a rust match pattern to a java boolean condition over the
     * scrutinee variable. Returns "true" for wildcards (always-match),
     * a java boolean expression otherwise, or "true" as best-effort fallback.
     */
    private static String patternToCondition(String pattern, String scrutVar) {
        String t = pattern.trim();
        // Strip guard (`Some(v) if cond`) — guards aren't directly
        // representable in a chained-ternary form; treat as condition.
        String guardCond = null;
        int ifIdx = t.indexOf(" if ");
        if (ifIdx > 0) {
            guardCond = t.substring(ifIdx + 4).trim();
            t = t.substring(0, ifIdx).trim();
        }
        String baseCond;
        if ("_".equals(t)) {
            baseCond = "true";
        } else if ("None".equals(t)) {
            baseCond = "(" + scrutVar + " == null)";
        } else if (t.matches("^(Some|Ok)\\s*\\(.*\\)$")) {
            // Truthy unwrap — value present.
            baseCond = "(" + scrutVar + " != null)";
        } else if (t.matches("^Err\\s*\\(.*\\)$")) {
            // Err in a Result<T,E> erased to T-or-null: the error path
            // is reached when the value IS null. Without this
            // distinction Ok and Err arms had identical conditions,
            // making Err structurally unreachable in lowered output.
            baseCond = "(" + scrutVar + " == null)";
        } else if (t.matches("^\".*\"$")) {
            // String literal pattern → equality check.
            baseCond = "(" + scrutVar + " != null && " + scrutVar + ".equals(" + t + "))";
        } else if (t.matches("^-?\\d+(\\.\\d+)?$")) {
            // Numeric literal pattern.
            baseCond = "((" + scrutVar + ") == (" + t + "))";
        } else {
            // Bare identifier or unknown — wildcard-equivalent.
            baseCond = "true";
        }
        if (guardCond != null) {
            return baseCond + " && (" + guardCond + ")";
        }
        return baseCond;
    }

    /** Replace whole-word identifier `from` with `to` in text. */
    private static String replaceIdentifier(String text, String from, String to) {
        return text.replaceAll("(?<![A-Za-z0-9_])" + java.util.regex.Pattern.quote(from) + "(?![A-Za-z0-9_])", java.util.regex.Matcher.quoteReplacement(to));
    }

    private static String localDeclaration(String returnType, String name, String expression, boolean alreadyDefined) {
        if (alreadyDefined) {
            return name + " = " + expression + ";";
        }
        String type = mapSourceType(returnType);
        // Use `var` for type inference when the type isn't known or would
        // be wrong (void function with non-void value, Object fallback).
        // Java 10+ supports var for local variables; the compiler infers
        // from RHS, which is correct for `let x = call_returning_String()`.
        if ("void".equals(type) || "Object".equals(type) || type.isBlank()) {
            return "var " + name + " = " + expression + ";";
        }
        return type + " " + name + " = " + expression + ";";
    }

    private static String shapeConceptName(Jcs.Obj shape) {
        String value = shape.stringFieldOrNull("concept_name");
        if (value == null || value.isBlank()) {
            value = shape.stringFieldOrNull("conceptName");
        }
        return value == null ? "" : value.strip();
    }

    private static List<Jcs.Obj> shapeArgs(Jcs.Obj shape) {
        Jcs.Json value = shape.get("args");
        if (!(value instanceof Jcs.Arr arr)) {
            return List.of();
        }
        List<Jcs.Obj> out = new ArrayList<>();
        for (Jcs.Json child : arr.values()) {
            if (child instanceof Jcs.Obj obj) {
                out.add(obj);
            }
        }
        return out;
    }

    private static Map<List<Integer>, String> operandBindingMap(String operandBindingsJson) {
        Map<List<Integer>, String> out = new TreeMap<>(SugarRealizer::comparePosition);
        if (operandBindingsJson == null || operandBindingsJson.isBlank() || "[]".equals(operandBindingsJson.trim())) {
            return out;
        }
        Jcs.Json parsed;
        try {
            parsed = Jcs.parse(operandBindingsJson);
        } catch (IllegalArgumentException e) {
            return out;
        }
        if (!(parsed instanceof Jcs.Arr arr)) {
            return out;
        }
        for (Jcs.Json item : arr.values()) {
            if (!(item instanceof Jcs.Obj obj)) {
                continue;
            }
            Jcs.Json rawPosition = obj.get("position");
            String symbol = obj.stringFieldOrNull("symbol");
            if (!(rawPosition instanceof Jcs.Arr parts) || symbol == null || symbol.isBlank()) {
                continue;
            }
            List<Integer> position = new ArrayList<>();
            boolean valid = true;
            for (Jcs.Json part : parts.values()) {
                if (part instanceof Jcs.Num n && n.value() >= 0 && n.value() <= Integer.MAX_VALUE) {
                    position.add((int) n.value());
                } else {
                    valid = false;
                    break;
                }
            }
            if (valid) {
                out.put(List.copyOf(position), symbol);
            }
        }
        return out;
    }

    private static int comparePosition(List<Integer> left, List<Integer> right) {
        int count = Math.min(left.size(), right.size());
        for (int i = 0; i < count; i++) {
            int cmp = Integer.compare(left.get(i), right.get(i));
            if (cmp != 0) {
                return cmp;
            }
        }
        return Integer.compare(left.size(), right.size());
    }

    private static List<Integer> appendPosition(List<Integer> position, int next) {
        List<Integer> out = new ArrayList<>(position);
        out.add(next);
        return List.copyOf(out);
    }

    private static ShapeExpression symbolTerm(String symbol, ShapeContext context) {
        if ("true".equals(symbol) || "True".equals(symbol)) {
            return new ShapeExpression("true", "boolean");
        }
        if ("false".equals(symbol) || "False".equals(symbol)) {
            return new ShapeExpression("false", "boolean");
        }
        if ("None".equals(symbol) || "null".equals(symbol)) {
            return new ShapeExpression("null", "Object");
        }
        if (symbol.matches("-?[0-9]+")) {
            return new ShapeExpression(symbol, "int");
        }
        if (symbol.length() >= 2 && symbol.startsWith("\"") && symbol.endsWith("\"")) {
            return new ShapeExpression(symbol, "String");
        }
        return new ShapeExpression(symbol, typeForArgument(symbol, context));
    }

    private static ShapeExpression literalTerm(Jcs.Json value) {
        if (value instanceof Jcs.Bool b) {
            return new ShapeExpression(Boolean.toString(b.value()), "boolean");
        }
        if (value instanceof Jcs.Num n) {
            return new ShapeExpression(Long.toString(n.value()), "int");
        }
        if (value instanceof Jcs.Str s) {
            return new ShapeExpression(JsonUtil.quoted(s.value()), "String");
        }
        if (value instanceof Jcs.Null || value == null) {
            return new ShapeExpression("null", "Object");
        }
        return new ShapeExpression("null", "Object");
    }

    private static String typeForArgument(String symbol, ShapeContext context) {
        for (int i = 0; i < context.params.size(); i++) {
            if (context.params.get(i).equals(symbol)) {
                return i < context.paramTypes.size() ? mapSourceType(context.paramTypes.get(i)) : "";
            }
        }
        // For unknown symbols (not a param), return blank — let callers
        // fall back to var inference / valueToTree wrap. Falling back to
        // context.returnType was wrong: it caused json! macros to treat
        // `adapter.name()` (String result) as the function's return type
        // (JsonNode), choosing the wrong .set/.put branch.
        return "";
    }

    private static boolean isIdentifier(String value) {
        return value != null && value.matches("[A-Za-z_$][A-Za-z0-9_$]*");
    }

    private static String indentBlock(String body) {
        if (body.isBlank()) {
            return "    ;";
        }
        return body.lines().map(line -> "    " + line).collect(Collectors.joining("\n"));
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
     * Per-invocation library_tag from the dispatcher. Set by emitStub at the
     * start of a call and cleared at the end. Body-template lookup uses this
     * to disambiguate when multiple libraries ship templates for the same
     * concept — without it, selection is load-order-dependent. Empty string
     * means "library-agnostic" (legacy callers, classpath catch-all).
     */
    private static final ThreadLocal<String> CURRENT_LIBRARY_TAG = ThreadLocal.withInitial(() -> "");

    /**
     * #1369: parametric content-addressing. Carries the (composite_cid →
     * ParametricSortExpansion) map for the current realize invocation so
     * mapConceptHubSortCidToJava can decompose composite CIDs into
     * (constructor, args) for parameterized morphism dispatch.
     */
    record ParametricExpansion(String cid, String constructorCid, java.util.List<String> argCids) {}
    private static final ThreadLocal<java.util.Map<String, ParametricExpansion>> CURRENT_EXPANSIONS =
        ThreadLocal.withInitial(java.util.HashMap::new);

    // Substrate-canonical constructor CIDs for parametric dispatch.
    private static final String REF_T_CONSTRUCTOR_CID =
        "blake3-512:37d8efe0ce6321d1a16f80aa06cbdf056c846b8a99613731e8d64d9581af61bc517fd8c87daaff2c817585a7dfd763e09ed729fdc71d25fe16fb1b2e6ca33534";
    private static final String LIST_T_CONSTRUCTOR_CID =
        "blake3-512:e3f8d17445f9d2ce89c41c09cbeea08a8bc685d1c34a9fd3dfa7b1df17a94f40eab37396615501f1468baf2a1480fd5a27330ea23202b99876c5f4d97fa2cfb2";

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
            Integer maxParams,
            /// Substrate-honest library_tag pin. Captured from the body-template
            /// entry's `target_library_tag` field at parse time. Used by the
            /// matcher to disambiguate when multiple libraries ship templates
            /// for the same concept. Empty string means "library-agnostic"
            /// (legacy catch-all).
            String targetLibraryTag,
            /// #1390: static field helpers lifted from the shim's source file.
            /// Bodies reference these as short-named symbols (e.g.
            /// `MAPPER.readTree(s)`); the assembler hoists them into the
            /// compilation unit before methods. Empty when the shim has no
            /// class-level static fields.
            List<String> fileHelpers) {}

    private record TemplateCitation(
            String placeholder,
            String conceptName,
            String mode,
            List<String> params) {}

    private record RenderedBody(String body, Jcs.Json lossRecord, List<String> helpers) {
        RenderedBody(String body, Jcs.Json lossRecord) {
            this(body, lossRecord, List.of());
        }
    }

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
        // Library-tag-keyed selection: two ordered passes to remove
        // load-order dependency.
        //
        // Pass 1 (only when currentLib is non-empty): try entries whose
        // target_library_tag equals currentLib exactly. This is the
        // intended library's body templates.
        //
        // Pass 2 (always): try entries whose target_library_tag is empty
        // (library-agnostic / classpath catch-all).
        //
        // Entries from OTHER libraries are never considered. When
        // currentLib is empty (legacy / untagged callers), we skip pass 1
        // entirely so the load-order-dependent selection that was the
        // original bug stays closed.
        String currentLib = CURRENT_LIBRARY_TAG.get();
        if (!currentLib.isEmpty()) {
            Optional<RenderedBody> exact = tryEntries(entries, conceptName, mode, params,
                    recursionStack, e -> e.targetLibraryTag().equals(currentLib));
            if (exact.isPresent()) return exact;
        }
        return tryEntries(entries, conceptName, mode, params, recursionStack,
                e -> e.targetLibraryTag().isEmpty());
    }

    private static Optional<RenderedBody> tryEntries(
            List<BodyTemplateEntry> entries,
            String conceptName,
            String mode,
            List<String> params,
            List<String> recursionStack,
            java.util.function.Predicate<BodyTemplateEntry> filter) {
        for (BodyTemplateEntry e : entries) {
            if (!filter.test(e)) continue;
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
        // #1390: attach the matched entry's static field helpers to the
        // rendered body so emitStubInner / RpcServer can carry them through
        // to the assembler. Empty when the shim has no class-level statics.
        return Optional.of(new RenderedBody(rendered, lossRecord, entry.fileHelpers()));
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
        // Default classpath load: java-canonical-bodies.json (catch-all entries).
        List<BodyTemplateEntry> classpath = loadEntriesFromClasspath("java-canonical-bodies.json");
        // #1361 chunk 2 part B follow-up / #1355: also scan filesystem for
        // per-library-tag body-templates JSONs. Each sister shim (e.g.
        // provekit-shim-stdio-java) auto-generates one of these on mint;
        // without this load path, RESOLVE'd boundaries would refuse with
        // is_stub even when the substrate has the body-template ready.
        // Same pattern the rust realize binary uses (load_library_body_template
        // walking menagerie/<lang>-language-signature/specs/body-templates/).
        List<BodyTemplateEntry> filesystem = loadEntriesFromFilesystem();
        // Merge: classpath entries first (catch-all), then per-library entries
        // (more specific). Order matters because conceptMatches is first-match-wins
        // in callers; per-library entries override catch-all for the same concept.
        List<BodyTemplateEntry> merged = new ArrayList<>(filesystem);
        merged.addAll(classpath);
        return merged;
    }

    /// Walk menagerie/java-language-signature/specs/body-templates/ for any
    /// java-canonical-bodies-<library_tag>.json files and load their entries.
    /// Resolves workspace root by walking up from CWD looking for
    /// `menagerie/` directory.
    private static List<BodyTemplateEntry> loadEntriesFromFilesystem() {
        java.nio.file.Path cwd = java.nio.file.Paths.get(System.getProperty("user.dir", "."));
        java.nio.file.Path root = null;
        for (java.nio.file.Path p = cwd; p != null; p = p.getParent()) {
            if (java.nio.file.Files.isDirectory(p.resolve("menagerie"))) {
                root = p;
                break;
            }
        }
        if (root == null) {
            return List.of();
        }
        java.nio.file.Path dir = root.resolve("menagerie")
                .resolve("java-language-signature")
                .resolve("specs")
                .resolve("body-templates");
        if (!java.nio.file.Files.isDirectory(dir)) {
            return List.of();
        }
        List<BodyTemplateEntry> out = new ArrayList<>();
        try (java.util.stream.Stream<java.nio.file.Path> files = java.nio.file.Files.list(dir)) {
            for (java.nio.file.Path file : (Iterable<java.nio.file.Path>) files::iterator) {
                String name = file.getFileName().toString();
                if (!name.startsWith("java-canonical-bodies-") || !name.endsWith(".json")) {
                    continue;
                }
                try {
                    String raw = java.nio.file.Files.readString(file, StandardCharsets.UTF_8);
                    out.addAll(parseEntriesFromRaw(raw));
                } catch (IOException ignore) {
                    // Single bad file: degrade silently. Other files still load.
                }
            }
        } catch (IOException ignore) {
            return List.of();
        }
        return out;
    }

    private static List<BodyTemplateEntry> loadEntriesFromClasspath(String resource) {
        try (InputStream in = SugarRealizer.class.getResourceAsStream(resource)) {
            if (in == null) {
                return List.of();
            }
            String raw;
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8))) {
                raw = reader.lines().collect(Collectors.joining("\n"));
            }
            return parseEntriesFromRaw(raw);
        } catch (IOException e) {
            return List.of();
        }
    }

    private static List<BodyTemplateEntry> parseEntriesFromRaw(String raw) {
        try {
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
                String entryLibraryTag = itemObj.stringFieldOrNull("target_library_tag");
                if (entryLibraryTag == null) entryLibraryTag = "";
                // #1390: static field helpers lifted from the shim's source.
                List<String> fileHelpers = new ArrayList<>();
                Jcs.Json helpersJson = itemObj.get("file_helpers");
                if (helpersJson instanceof Jcs.Arr helpersArr) {
                    for (Jcs.Json v : helpersArr.values()) {
                        if (v instanceof Jcs.Str s) fileHelpers.add(s.value());
                    }
                }
                out.add(new BodyTemplateEntry(
                        conceptName,
                        mode,
                        kind,
                        tmpl,
                        citations.get(),
                        lossRecordValue(itemObj),
                        minParams,
                        maxParams,
                        entryLibraryTag,
                        fileHelpers));
            }
            return out;
        } catch (RuntimeException e) {
            // JSON parse failure: degrade to "no entries"; stubs will emit.
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

record TransportedOperation(
        String conceptCid,
        String conceptSiteCid,
        String lossRecordCid,
        String operationKind,
        String policyCid,
        String shapeCid,
        List<Integer> termPosition,
        Jcs.Json argsJcs,
        String argsJcsCid,
        String sugarDictCid,
        String callsiteCid,
        String conceptName,
        String targetLibraryTag) {
    TransportedOperation {
        conceptCid = conceptCid == null ? "" : conceptCid;
        conceptSiteCid = conceptSiteCid == null ? "" : conceptSiteCid;
        lossRecordCid = lossRecordCid == null ? "" : lossRecordCid;
        operationKind = operationKind == null ? "" : operationKind;
        policyCid = policyCid == null ? "" : policyCid;
        shapeCid = shapeCid == null ? "" : shapeCid;
        termPosition = termPosition == null ? List.of() : List.copyOf(termPosition);
        argsJcsCid = argsJcsCid == null ? "" : argsJcsCid;
        sugarDictCid = sugarDictCid == null ? "" : sugarDictCid;
        callsiteCid = callsiteCid == null ? "" : callsiteCid;
        conceptName = conceptName == null ? "" : conceptName;
        targetLibraryTag = targetLibraryTag == null ? "" : targetLibraryTag;
    }

    static TransportedOperation fromJson(String json) {
        if (json == null || json.isBlank() || "{}".equals(json.trim())) {
            return null;
        }
        Jcs.Json parsed;
        try {
            parsed = Jcs.parse(json);
        } catch (IllegalArgumentException e) {
            return null;
        }
        if (!(parsed instanceof Jcs.Obj obj)) {
            return null;
        }
        return new TransportedOperation(
                stringValue(obj, "concept_cid", "conceptCid"),
                stringValue(obj, "concept_site_cid", "conceptSiteCid"),
                stringValue(obj, "loss_record_cid", "lossRecordCid"),
                stringValue(obj, "operation_kind", "operationKind"),
                stringValue(obj, "policy_cid", "policyCid"),
                stringValue(obj, "shape_cid", "shapeCid"),
                termPosition(value(obj, "term_position", "termPosition")),
                value(obj, "args_jcs", "argsJcs"),
                stringValue(obj, "args_jcs_cid", "argsJcsCid"),
                stringValue(obj, "sugar_dict_cid", "sugarDictCid"),
                stringValue(obj, "callsite_cid", "callsiteCid"),
                stringValue(obj, "concept_name", "conceptName"),
                stringValue(obj, "target_library_tag", "targetLibraryTag")
        );
    }

    static TransportedOperation fromNamedTermTree(String json) {
        if (json == null || json.isBlank() || "{}".equals(json.trim())) {
            return null;
        }
        Jcs.Json parsed;
        try {
            parsed = Jcs.parse(json);
        } catch (IllegalArgumentException e) {
            return null;
        }
        if (!(parsed instanceof Jcs.Obj obj)) {
            return null;
        }

        String conceptName = obj.stringFieldOrNull("conceptName");
        String operationKind = obj.stringFieldOrNull("operationKind");
        if (!isJavaCarrierConcept(conceptName, operationKind)) {
            return null;
        }
        if (operationKind == null || operationKind.isBlank()) {
            operationKind = conceptNameMatches(conceptName, "addr") ? "addr" : "deref";
        }
        if (conceptName == null || conceptName.isBlank()) {
            conceptName = "concept:" + operationKind;
        }
        String shapeCid = obj.stringFieldOrNull("shapeCid");
        if (shapeCid == null || shapeCid.isBlank()) {
            return null;
        }

        Jcs.Arr argsJcs = obj.get("args") instanceof Jcs.Arr args ? args : Jcs.array();
        List<Integer> termPosition = List.of(0);
        Jcs.Obj site = Jcs.object(
                "args_jcs_cid", Jcs.string(Jcs.cid(argsJcs)),
                "concept_cid", Jcs.string(shapeCid),
                "concept_name", Jcs.string(conceptName == null ? "" : conceptName),
                "operation_kind", Jcs.string(operationKind == null ? "" : operationKind),
                "shape_cid", Jcs.string(shapeCid),
                "term_position", Jcs.array(termPosition.stream().map(Jcs::integer).toList())
        );
        Jcs.Json lossRecord = javaCarrierLossRecord(conceptName, operationKind);
        return new TransportedOperation(
                shapeCid,
                Jcs.cid(site),
                Jcs.cid(lossRecord),
                operationKind,
                "",
                shapeCid,
                termPosition,
                argsJcs,
                "",
                "",
                "",
                conceptName,
                "java"
        );
    }

    private static boolean isJavaCarrierConcept(String conceptName, String operationKind) {
        return conceptNameMatches(conceptName, "addr")
                || conceptNameMatches(conceptName, "deref")
                || "addr".equals(operationKind)
                || "deref".equals(operationKind);
    }

    private static boolean conceptNameMatches(String conceptName, String bareName) {
        if (conceptName == null) {
            return false;
        }
        return conceptName.equals(bareName) || conceptName.equals("concept:" + bareName);
    }

    private static Jcs.Json javaCarrierLossRecord(String conceptName, String operationKind) {
        String contribution;
        if (conceptNameMatches(conceptName, "addr") || "addr".equals(operationKind)) {
            contribution = "java-references-not-addresses";
        } else if (conceptNameMatches(conceptName, "deref") || "deref".equals(operationKind)) {
            contribution = "java-implicit-deref";
        } else {
            return Jcs.object();
        }
        return Jcs.object(
                contribution, Jcs.object(
                        "args", Jcs.array(),
                        "head", Jcs.string("atomic"),
                        "name", Jcs.string(contribution)
                )
        );
    }

    private static Jcs.Json value(Jcs.Obj obj, String... keys) {
        for (String key : keys) {
            Jcs.Json value = obj.get(key);
            if (value != null) {
                return value;
            }
        }
        return null;
    }

    private static String stringValue(Jcs.Obj obj, String... keys) {
        Jcs.Json value = value(obj, keys);
        return value instanceof Jcs.Str s ? s.value() : "";
    }

    private static List<Integer> termPosition(Jcs.Json value) {
        if (!(value instanceof Jcs.Arr arr)) {
            return List.of();
        }
        List<Integer> out = new ArrayList<>();
        for (Jcs.Json item : arr.values()) {
            if (!(item instanceof Jcs.Num number) || number.value() < 0 || number.value() > Integer.MAX_VALUE) {
                return List.of();
            }
            out.add((int) number.value());
        }
        return out;
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
