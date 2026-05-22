package com.provekit.realize;

import com.provekit.ir.Blake3;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

public final class RpcServer {
    // PEP 1.7.0 sugar plugin CID for java-canonical.
    // Computed by compute_plugin_cid() over the java-canonical.json content.
    // Update this value if java-canonical.json content changes.
    static final String PLUGIN_CID =
        "blake3-512:b7ad1160f00d892d310fb33ac3372a4ebb2f89fec563cab1719e7006ab3d7593aae2162b882aedbec1b97e44957240b3c7e8ab1675456f0539c4ad3f45d22a7b";

    private final BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    private final PrintWriter out = new PrintWriter(System.out, true);
    private final JavaNullBoundaryRealizer realizer = new JavaNullBoundaryRealizer();

    public void run() {
        try {
            String line;
            while ((line = in.readLine()) != null) {
                handle(line.trim());
            }
        } catch (IOException e) {
            System.err.println("ORP RPC read error: " + e.getMessage());
        }
    }

    private void handle(String line) {
        if (line.isEmpty()) return;
        String id = JsonUtil.extractId(line);
        String method = JsonUtil.extractMethod(line);
        try {
            switch (method) {
                // PEP 1.7.0 methods
                case "provekit.plugin.describe" -> sendResponse(id, describeResult());
                case "provekit.plugin.platform_semantics" ->
                    sendResponse(id, PlatformSemanticsDeclaration.toJson());
                case "provekit.plugin.literal_encoding_answers" ->
                    sendResponse(id, LiteralEncodingAnswers.toJson());
                case "provekit.plugin.invoke" -> {
                    // handleInvoke returns a full JSON object: {"source":..., "is_stub":...}
                    String resultObj = handleInvoke(line);
                    sendResponse(id, resultObj);
                }
                // #1375 Milestone C: target-owned assembly. Substrate sends a
                // batch of fragments + a destination hint; java decides file
                // layout (package, imports, class wrapping, helper placement)
                // and returns the files to write. Substrate stops baking
                // java's file syntax.
                case "provekit.plugin.assemble" -> {
                    String resultObj = handleAssemble(line);
                    sendResponse(id, resultObj);
                }
                case "provekit.plugin.shutdown" -> {
                    sendResponse(id, "null");
                    System.exit(0);
                }
                // ORP v1 methods (backward compatibility)
                case "initialize" -> sendResponse(id, initResult());
                case "realize" -> {
                    RealizerPlan plan = RealizerPlan.fromJsonLine(line);
                    RealizerOutput output = realizer.realize(plan);
                    sendResponse(id, "{\"output\":" + output.toJson() + "}");
                }
                case "shutdown" -> {
                    sendResponse(id, "null");
                    System.exit(0);
                }
                default -> sendError(id, -32601, "unknown method: " + method);
            }
        } catch (Exception e) {
            sendError(id, -32000, e.getMessage() != null ? e.getMessage() : e.getClass().getName());
        }
    }

    /**
     * Handle provekit.plugin.invoke.
     *
     * Params (from the JSON-RPC request "params" object):
     *   function      - snake_case function name
     *   params        - JSON array of parameter name strings
     *   param_types   - JSON array of source-language type strings
     *   return_type   - source-language return type string
     *   concept_name  - concept binding name for annotation + stub body
     *
     * Returns: JSON object with `source` (Java string) and `is_stub` (boolean).
     * `is_stub=true` means the body fell through to the language stub
     * (no body-template matched); `is_stub=false` means a body-template
     * entry rendered a real body. cmd_bind uses this to emit accurate
     * per-concept `bind-stub-body-emitted` gap entries per body-template-memento.md §5.
     */
    /**
     * #1375 Milestone C: target-owned compilation-unit assembly.
     *
     * Substrate sends the kit the fragments it collected for one source
     * file + a destination hint (file_basename, optional package_hint).
     * The kit decides:
     *   - file names (may split into multiple files)
     *   - package declaration
     *   - import block (dedupe across fragments)
     *   - class wrapping (one or many)
     *   - helper placement (static fields, init blocks)
     *
     * Returns a list of {path, content} pairs that the substrate writes
     * verbatim to the out-dir. The substrate stops baking java's file
     * syntax — that decision lives here now.
     *
     * Request shape:
     *   {"target_lang":"java","file_basename":"lib","package_hint":"...",
     *    "fragments":[{
     *       "concept_name":"...",
     *       "source":"...",
     *       "imports":[...],
     *       "helpers":[...],
     *       "dependencies":[...],
     *       "diagnostics":[...],
     *       "compile_unit_requirements":{...}
     *    }, ...]}
     *
     * Response shape:
     *   {"files":[{"path":"Lib.java","content":"..."}]}
     */
    private String handleAssemble(String line) {
        String paramsObj = JsonUtil.extractParamsObject(line);
        String fileBasename = JsonUtil.decodeJsonStringField(paramsObj, "file_basename");
        if (fileBasename == null || fileBasename.isBlank()) fileBasename = "lib";
        String packageHint = JsonUtil.decodeJsonStringField(paramsObj, "package_hint");
        String fragmentsJson = JsonUtil.extractArrayField(paramsObj, "fragments");

        // Parse fragments + collect imports/sources.
        java.util.TreeSet<String> mergedImports = new java.util.TreeSet<>();
        java.util.List<String> bodies = new java.util.ArrayList<>();
        try {
            com.provekit.ir.Jcs.Json doc = com.provekit.ir.Jcs.parse(fragmentsJson);
            if (doc instanceof com.provekit.ir.Jcs.Arr arr) {
                for (com.provekit.ir.Jcs.Json item : arr.values()) {
                    if (!(item instanceof com.provekit.ir.Jcs.Obj o)) continue;
                    String src = o.stringFieldOrNull("source");
                    if (src != null && !src.isBlank()) bodies.add(src);
                    com.provekit.ir.Jcs.Json importsArr = o.get("imports");
                    if (importsArr instanceof com.provekit.ir.Jcs.Arr ia) {
                        for (com.provekit.ir.Jcs.Json v : ia.values()) {
                            if (v instanceof com.provekit.ir.Jcs.Str s) {
                                String fqn = s.value();
                                if (!fqn.startsWith("java.lang.")) mergedImports.add(fqn);
                            }
                        }
                    }
                }
            }
        } catch (RuntimeException ignored) {
            // Substrate-honest: malformed fragments → empty compilation unit.
        }

        // Class name: PascalCase from file basename.
        String className = toPascalCase(fileBasename);
        StringBuilder out = new StringBuilder();
        if (packageHint != null && !packageHint.isBlank()) {
            out.append("package ").append(packageHint).append(";\n\n");
        }
        for (String imp : mergedImports) {
            out.append("import ").append(imp).append(";\n");
        }
        if (!mergedImports.isEmpty()) out.append('\n');
        out.append("public final class ").append(className).append(" {\n");
        for (int i = 0; i < bodies.size(); i++) {
            String body = bodies.get(i);
            // Strip outer wrapper class if the fragment came pre-wrapped.
            // Realizers historically emit `final class FooTransported { method }`;
            // the assembler peels that to get just the methods. Detected by
            // a `class` keyword followed by `{` on the first non-comment line.
            String unwrapped = stripWrappingClass(body);
            for (String line2 : unwrapped.split("\n", -1)) {
                if (line2.isEmpty()) {
                    out.append('\n');
                } else {
                    out.append("    ").append(line2).append('\n');
                }
            }
            if (i + 1 < bodies.size()) out.append('\n');
        }
        out.append("}\n");

        // Single file response for now.
        String filePath = className + ".java";
        return "{\"files\":[{"
            + "\"path\":" + JsonUtil.quoted(filePath)
            + ",\"content\":" + JsonUtil.quoted(out.toString())
            + "}]}";
    }

    /** PascalCase from snake-case or kebab-case file basename. */
    private static String toPascalCase(String basename) {
        StringBuilder sb = new StringBuilder();
        boolean upNext = true;
        for (int i = 0; i < basename.length(); i++) {
            char c = basename.charAt(i);
            if (c == '_' || c == '-' || c == '.') {
                upNext = true;
            } else if (upNext) {
                sb.append(Character.toUpperCase(c));
                upNext = false;
            } else {
                sb.append(c);
            }
        }
        return sb.length() == 0 ? "Lib" : sb.toString();
    }

    /**
     * Strip an outer `final class Foo { ... }` wrapper if present, returning
     * just the inner body. Realizers historically wrap each method in a
     * per-concept final class; the assembler collects them into one outer
     * class, so the inner wrappers must be peeled.
     *
     * Returns the original body unchanged if no wrapper is detected.
     */
    private static String stripWrappingClass(String body) {
        String trimmed = body.trim();
        // Look for "final class <Name> {" near the start, optionally
        // preceded by comments.
        java.util.regex.Pattern p = java.util.regex.Pattern.compile(
            "(?s)^\\s*(?://[^\\n]*\\n\\s*)*(?:final\\s+|public\\s+)?class\\s+\\w+\\s*\\{(.*)\\}\\s*$"
        );
        java.util.regex.Matcher m = p.matcher(trimmed);
        if (m.matches()) {
            return m.group(1).trim();
        }
        return body;
    }

    private String handleInvoke(String line) {
        // Extract the inner params object to avoid ambiguity with the RPC "params" key.
        String paramsObj = JsonUtil.extractParamsObject(line);
        String function = JsonUtil.decodeJsonStringField(paramsObj, "function");
        String sourceFunctionName = JsonUtil.decodeJsonStringField(paramsObj, "source_function_name");
        if (sourceFunctionName.isBlank()) {
            sourceFunctionName = JsonUtil.decodeJsonStringField(paramsObj, "sourceFunctionName");
        }
        String emittedFunction = sourceFunctionName.isBlank() ? function : sourceFunctionName;
        String returnType = JsonUtil.decodeJsonStringField(paramsObj, "return_type");
        String conceptName = JsonUtil.decodeJsonStringField(paramsObj, "concept_name");
        String mode = JsonUtil.decodeJsonStringField(paramsObj, "mode");
        List<String> modes = JsonUtil.decodeJsonStringArray(paramsObj, "modes");
        ContractPayload contract = ContractPayload.fromJson(JsonUtil.extractObjectField(paramsObj, "contract"));
        TransportedOperation transportedOp = TransportedOperation.fromJson(JsonUtil.extractObjectField(paramsObj, "transported_op"));
        if (transportedOp == null) {
            String namedTermTree = JsonUtil.extractObjectField(paramsObj, "named_term_tree");
            if ("{}".equals(namedTermTree)) {
                namedTermTree = JsonUtil.extractObjectField(paramsObj, "namedTermTree");
            }
            transportedOp = TransportedOperation.fromNamedTermTree(namedTermTree);
        }
        String termShape = JsonUtil.extractObjectField(paramsObj, "term_shape");
        if ("{}".equals(termShape)) {
            termShape = JsonUtil.extractObjectField(paramsObj, "termShape");
        }
        String operandBindings = JsonUtil.extractArrayField(paramsObj, "operand_bindings");
        if ("[]".equals(operandBindings)) {
            operandBindings = JsonUtil.extractArrayField(paramsObj, "operandBindings");
        }
        List<String> sugarPlugins = JsonUtil.decodeJsonObjectArray(paramsObj, "sugar_plugins");
        List<String> params = JsonUtil.decodeJsonStringArray(paramsObj, "params");
        List<String> paramTypes = JsonUtil.decodeJsonStringArray(paramsObj, "param_types");
        // Substrate-honest cross-language signature pins: concept-hub sort
        // CIDs flow through the carrier from the SOURCE kit's lift. The
        // target (java) realize binary uses them to resolve java syntax
        // via its own catalog — no per-(source, target) translation table.
        List<String> paramSortCids = JsonUtil.decodeJsonStringArray(paramsObj, "param_sort_cids");
        String returnSortCid = JsonUtil.decodeJsonStringField(paramsObj, "return_sort_cid");
        if (returnSortCid == null) returnSortCid = "";
        // Cross-language signaling discriminator: explicit field presence on
        // the RPC params object. EITHER param_sort_cids OR return_sort_cid
        // declared in the payload → caller is cross-lang and any empty CID
        // means "substrate gap; refuse loudly". Field-absent means same-lang
        // / legacy → empties are absence-of-signal, not declared gap.
        boolean isCrossLang = JsonUtil.hasField(paramsObj, "param_sort_cids")
                || JsonUtil.hasField(paramsObj, "return_sort_cid");
        // Dispatcher-resolved library_tag for body-template disambiguation.
        // Absent → "" → matcher only considers library-agnostic catch-all entries.
        String targetLibraryTag = JsonUtil.decodeJsonStringField(paramsObj, "target_library_tag");
        if (targetLibraryTag == null) targetLibraryTag = "";
        // #1369: parametric content-addressing expansions for composite sort CIDs.
        // Each expansion declares (composite_cid → constructor_cid + arg_cids)
        // so SugarRealizer can decompose composite CIDs for parameterized
        // morphism dispatch.
        java.util.List<SugarRealizer.ParametricExpansion> parametricExpansions = parseParametricExpansions(
                JsonUtil.extractArrayField(paramsObj, "parametric_sort_expansions"));
        SugarRealizer.Realization r =
                SugarRealizer.emitStub(emittedFunction, params, paramTypes, paramSortCids, returnType, returnSortCid,
                        conceptName, mode, modes, contract, sugarPlugins, transportedOp, termShape, operandBindings,
                        isCrossLang, targetLibraryTag, parametricExpansions);
        String wrapperRecord = r.observationWrapperEmissionRecord() == null
                ? ""
                : ",\"observation_wrapper_emission_record\":" + r.observationWrapperEmissionRecord();
        // #1374: extract FQN imports from the emitted source. Substrate-side
        // assembly (Milestone C) deduplicates these and emits the idiomatic
        // import block for the target language. The body itself can keep
        // FQN-inline references (compiles either way); the imports field
        // lets downstream tooling know what the fragment USES.
        String importsJson = importsFromSource(r.source());
        return "{\"kind\":\"realization-fragment\""
                + ",\"source\":" + JsonUtil.quoted(r.source())
                + ",\"emitted_artifact_cid\":"
                + JsonUtil.quoted(Blake3.blake3_512(r.source().getBytes(StandardCharsets.UTF_8)))
                + ",\"is_stub\":" + (r.isStub() ? "true" : "false")
                + ",\"observed_loss_record\":" + r.observedLossRecord()
                + ",\"used_sugars\":" + r.usedSugarsJson()
                + ",\"imports\":" + importsJson
                + wrapperRecord
                + "}";
    }

    /**
     * #1374: extract java FQN imports from the emitted source.
     *
     * Pattern: lowercase package segments separated by dots, then a
     * PascalCase class name. Matches `com.fasterxml.jackson.databind.JsonNode`,
     * `java.util.List`, `java.io.ByteArrayOutputStream`. Skips inner class
     * suffixes (the matcher captures up to the FIRST PascalCase identifier;
     * `JsonNode.NumberType` matches just `JsonNode`).
     *
     * Returns a JSON array of unique FQN strings sorted lexicographically.
     */
    private static String importsFromSource(String source) {
        if (source == null || source.isEmpty()) return "[]";
        java.util.regex.Pattern p = java.util.regex.Pattern.compile(
            "\\b([a-z][a-z0-9_]*(?:\\.[a-z][a-z0-9_]*)+\\.[A-Z][A-Za-z0-9_]*)"
        );
        java.util.regex.Matcher m = p.matcher(source);
        java.util.TreeSet<String> imports = new java.util.TreeSet<>();
        while (m.find()) {
            String fqn = m.group(1);
            // Skip java.lang.* — implicit in every compilation unit.
            if (fqn.startsWith("java.lang.")) continue;
            imports.add(fqn);
        }
        StringBuilder sb = new StringBuilder("[");
        boolean first = true;
        for (String fqn : imports) {
            if (!first) sb.append(',');
            sb.append(JsonUtil.quoted(fqn));
            first = false;
        }
        sb.append(']');
        return sb.toString();
    }

    /**
     * PEP 1.7.0 provekit.plugin.describe result.
     *
     * Returns the java-canonical sugar plugin memento (without envelope/metadata;
     * those are loader-level fields). The result IS the plugin memento body per §4.2.1.
     */
    private String describeResult() {
        // The content payload mirrors java-canonical.json header.content.
        // The CID is pre-computed and matches the fixture file.
        return "{"
            + "\"envelope\":{"
            + "\"declaredAt\":\"2026-05-12T00:00:00.000Z\","
            + "\"signature\":\"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\","
            + "\"signer\":\"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\""
            + "},"
            + "\"header\":{"
            + "\"cid\":" + JsonUtil.quoted(PLUGIN_CID) + ","
            + "\"content\":" + contentJson() + ","
            + "\"critical\":false,"
            + "\"kind\":\"sugar\","
            + "\"protocol_versions\":[\"pep/1.7.0\"],"
            + "\"provenance_cid\":\"blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\","
            + "\"schemaVersion\":\"1\","
            + "\"version\":\"1.0.0\""
            + "},"
            + "\"metadata\":{"
            + "\"note\":\"Canonical Java annotation sugar dict for ProvekIt contract clause rendering.\","
            + "\"source_url\":\"menagerie/java-language-signature/specs/sugar/java-canonical.json\""
            + "}"
            + "}";
    }

    /**
     * Returns the JSON-serialized content payload for the java-canonical sugar dict.
     * Must be byte-identical to java-canonical.json header.content.
     */
    private String contentJson() {
        return "{"
            + "\"entries\":["
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} > ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"gt\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} >= ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"ge\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} < ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"lt\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} <= ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"le\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} == ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"eq\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @ensures(${lhs} > ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"ensures_gt\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @ensures(${lhs} == ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"ensures_eq\"}}"
            + "],"
            + "\"sugar_name\":\"canonical\","
            + "\"target_language\":\"java\""
            + "}";
    }

    private String initResult() {
        return "{"
            + "\"name\":\"provekit-realize-java\","
            + "\"version\":\"0.1.0\","
            + "\"protocol_version\":\"provekit-orp/1\","
            + "\"capabilities\":{"
            + "\"kits\":[\"java\"],"
            + "\"modes\":[\"transform\"],"
            + "\"obligationKinds\":[\"gap\"],"
            + "\"predicates\":[\"non_null\"],"
            + "\"surfaces\":[\"java-provekit-native\",\"java-spring-web\"]"
            + "}"
            + "}";
    }

    private void sendResponse(String id, String result) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}");
    }

    private void sendError(String id, int code, String message) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"error\":{\"code\":" + code + ",\"message\":" + JsonUtil.quoted(message) + "}}");
    }

    /**
     * Parse the JSON array string into a list of ParametricExpansion records.
     * Returns empty list on null / empty / parse failure (substrate-honest:
     * absent expansions just means no parametric CIDs to decompose).
     */
    private static java.util.List<SugarRealizer.ParametricExpansion> parseParametricExpansions(String json) {
        if (json == null || json.isBlank() || "[]".equals(json.trim())) return java.util.List.of();
        java.util.List<SugarRealizer.ParametricExpansion> out = new java.util.ArrayList<>();
        try {
            com.provekit.ir.Jcs.Json doc = com.provekit.ir.Jcs.parse(json);
            if (!(doc instanceof com.provekit.ir.Jcs.Arr arr)) return java.util.List.of();
            for (com.provekit.ir.Jcs.Json item : arr.values()) {
                if (!(item instanceof com.provekit.ir.Jcs.Obj o)) continue;
                String cid = o.stringFieldOrNull("cid");
                String ctor = o.stringFieldOrNull("constructor_cid");
                if (cid == null || ctor == null) continue;
                com.provekit.ir.Jcs.Json argsJson = o.get("arg_cids");
                java.util.List<String> argCids = new java.util.ArrayList<>();
                if (argsJson instanceof com.provekit.ir.Jcs.Arr argArr) {
                    for (com.provekit.ir.Jcs.Json a : argArr.values()) {
                        if (a instanceof com.provekit.ir.Jcs.Str s) argCids.add(s.value());
                    }
                }
                out.add(new SugarRealizer.ParametricExpansion(cid, ctor, argCids));
            }
        } catch (RuntimeException ignored) {
            // Substrate-honest: malformed expansion data → no decomposition possible.
        }
        return out;
    }
}
