package com.provekit.realize;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
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
                case "provekit.plugin.invoke" -> {
                    // handleInvoke returns a full JSON object: {"source":..., "is_stub":...}
                    String resultObj = handleInvoke(line);
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
    private String handleInvoke(String line) {
        // Extract the inner params object to avoid ambiguity with the RPC "params" key.
        String paramsObj = JsonUtil.extractParamsObject(line);
        String function = JsonUtil.decodeJsonStringField(paramsObj, "function");
        String returnType = JsonUtil.decodeJsonStringField(paramsObj, "return_type");
        String conceptName = JsonUtil.decodeJsonStringField(paramsObj, "concept_name");
        List<String> params = JsonUtil.decodeJsonStringArray(paramsObj, "params");
        List<String> paramTypes = JsonUtil.decodeJsonStringArray(paramsObj, "param_types");
        SugarRealizer.Realization r =
                SugarRealizer.emitStub(function, params, paramTypes, returnType, conceptName);
        return "{\"source\":" + JsonUtil.quoted(r.source())
                + ",\"is_stub\":" + (r.isStub() ? "true" : "false") + "}";
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
}
