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

    private static final class ShapeContext {
        final List<String> params;
        final List<String> paramTypes;
        final String returnType;
        final Map<List<Integer>, String> operandBindings;
        final Set<String> definedSymbols = new TreeSet<>();
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
        Optional<String> body = lowerShapeBody(shape, context, List.of());
        if (body.isEmpty()) {
            Optional<ShapeExpression> expression = lowerShapeExpression(shape, context, List.of());
            if (expression.isEmpty() || expression.get().text().isBlank()) {
                return Optional.empty();
            }
            body = Optional.of("return " + expression.get().text() + ";");
        }
        return Optional.of(new RenderedBody(body.get(), Jcs.object()));
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
                    return Optional.empty();
                }
                String temp = context.tempName();
                context.definedSymbols.add(temp);
                context.lastAssignedSymbol = temp;
                lines.add(localDeclaration(context.returnType, temp, expression.get().text(), false));
            }
            if (!"void".equals(mapSourceType(context.returnType))
                    && lines.stream().noneMatch(line -> line.strip().startsWith("return "))
                    && !context.lastAssignedSymbol.isBlank()) {
                lines.add("return " + context.lastAssignedSymbol + ";");
            }
            return Optional.of(String.join("\n", lines));
        }
        if (conceptMatches("concept:assign", conceptName) && args.size() == 2) {
            Optional<ShapeExpression> target = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            Optional<ShapeExpression> value = lowerShapeExpression(args.get(1), context, appendPosition(position, 1));
            if (target.isEmpty() || value.isEmpty() || !isIdentifier(target.get().text())) {
                return Optional.empty();
            }
            String name = target.get().text();
            boolean alreadyDefined = context.definedSymbols.contains(name);
            context.definedSymbols.add(name);
            context.lastAssignedSymbol = name;
            return Optional.of(localDeclaration(context.returnType, name, value.get().text(), alreadyDefined));
        }
        if (conceptMatches("concept:return", conceptName)) {
            if (args.isEmpty()) {
                return Optional.of("return;");
            }
            Optional<ShapeExpression> value = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            return value.map(shapeExpression -> "return " + shapeExpression.text() + ";");
        }
        if (conceptMatches("concept:conditional", conceptName) && args.size() == 3) {
            Optional<ShapeExpression> condition = lowerShapeExpression(args.get(0), context, appendPosition(position, 0));
            Optional<String> thenBody = lowerShapeBranchBody(args.get(1), context, appendPosition(position, 1));
            Optional<String> elseBody = lowerShapeBranchBody(args.get(2), context, appendPosition(position, 2));
            if (condition.isEmpty() || thenBody.isEmpty() || elseBody.isEmpty()) {
                return Optional.empty();
            }
            return Optional.of("if " + condition.get().text() + " {\n"
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
            if (!isIdentifier(callee)) {
                return Optional.empty();
            }
            String joined = argTerms.stream().skip(1).map(ShapeExpression::text).collect(Collectors.joining(", "));
            return Optional.of(new ShapeExpression(callee + "(" + joined + ")", mapSourceType(context.returnType)));
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
            String value = argTerms.get(0).text();
            return Optional.of(new ShapeExpression(
                    "(" + mapSourceType(typeName) + ") (" + value + ")",
                    mapSourceType(typeName)));
        }
        if (conceptMatches("concept:closure", conceptName) && !argTerms.isEmpty()) {
            // Best-effort: emit a java lambda. Rust closures get lifted as
            // concept:closure(param_leaf*, body). Java's lambda syntax is
            // (a, b) -> body. Skip captured-state semantics; this only
            // works for pure-function closures.
            int bodyIdx = argTerms.size() - 1;
            String params = argTerms.subList(0, bodyIdx).stream()
                    .map(ShapeExpression::text)
                    .collect(Collectors.joining(", "));
            String body = argTerms.get(bodyIdx).text();
            return Optional.of(new ShapeExpression(
                    "(" + params + ") -> " + body,
                    mapSourceType(context.returnType)));
        }
        // concept:reference (rust &x) and concept:deref (*x): java has neither
        // explicit reference nor deref operators — references are implicit on
        // objects; dereference is a no-op. Pass the inner expression through.
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
                case "eq" -> "(" + left + ") == (" + right + ")";
                case "ne" -> "(" + left + ") != (" + right + ")";
                case "lt" -> "(" + left + ") < (" + right + ")";
                case "le" -> "(" + left + ") <= (" + right + ")";
                case "gt" -> "(" + left + ") > (" + right + ")";
                case "ge" -> "(" + left + ") >= (" + right + ")";
                case "and" -> "(" + left + ") && (" + right + ")";
                case "or" -> "(" + left + ") || (" + right + ")";
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
            case "call" -> mapSourceType(fallbackReturnType);
            default -> args.isEmpty() ? mapSourceType(fallbackReturnType) : args.get(0).typeName();
        };
    }

    private static String localDeclaration(String returnType, String name, String expression, boolean alreadyDefined) {
        if (alreadyDefined) {
            return name + " = " + expression + ";";
        }
        String type = mapSourceType(returnType);
        if ("void".equals(type)) {
            type = "Object";
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
        return mapSourceType(context.returnType);
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
