package com.provekit.emit.junit;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;

import com.provekit.ir.Jcs;

/**
 * PEP 1.7.0 newline-delimited JSON-RPC server for the JUnit5 emitter plugin.
 *
 * <p>Reads one JSON-RPC request per line on stdin, writes one response per
 * line to stdout. Supported methods:
 * <ul>
 *   <li>{@code provekit.plugin.describe}  - plugin self-description.</li>
 *   <li>{@code provekit.plugin.invoke}    - emit a JUnit5 test class from an
 *       {@link EmitPlan} carried in {@code params}; returns an
 *       {@link JUnitEmitter.Emission}.</li>
 *   <li>{@code provekit.plugin.shutdown}  - exit.</li>
 * </ul>
 *
 * <p>Mirrors the RpcServer shape in {@code provekit-realize-java-core} but is
 * deliberately smaller: there is no body-emit, no assembly, no platform
 * semantics. The emitter is a predicate -> assertion table plus a
 * test-class shell.
 */
public final class RpcServer {
    private final BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    private final PrintWriter out = new PrintWriter(System.out, true);
    private final JUnitEmitter emitter = new JUnitEmitter();

    public void run() {
        try {
            String line;
            while ((line = in.readLine()) != null) {
                String trimmed = line.trim();
                if (!trimmed.isEmpty()) handle(trimmed);
            }
        } catch (IOException e) {
            System.err.println("emit-java-junit RPC read error: " + e.getMessage());
        }
    }

    private void handle(String line) {
        String id = "null";
        String method = "";
        String params = "{}";
        try {
            Jcs.Json doc = Jcs.parse(line);
            if (doc instanceof Jcs.Obj obj) {
                Jcs.Json idJson = obj.get("id");
                if (idJson instanceof Jcs.Num n) id = Long.toString(n.value());
                else if (idJson instanceof Jcs.Str s) id = "\"" + s.value() + "\"";
                method = obj.stringFieldOrNull("method");
                if (method == null) method = "";
                Jcs.Json p = obj.get("params");
                if (p instanceof Jcs.Obj) params = Jcs.encode(p);
            }
        } catch (RuntimeException e) {
            sendError(id, -32700, "parse error: " + e.getMessage());
            return;
        }

        try {
            switch (method) {
                case "provekit.plugin.describe" -> sendResponse(id, describeResult());
                case "provekit.plugin.invoke" -> {
                    EmitPlan plan = EmitPlan.fromParams(params);
                    JUnitEmitter.Emission emission = emitter.emit(plan);
                    sendResponse(id, emission.toJson());
                }
                case "provekit.plugin.shutdown" -> {
                    sendResponse(id, "null");
                    System.exit(0);
                }
                default -> sendError(id, -32601, "unknown method: " + method);
            }
        } catch (Exception e) {
            sendError(id, -32000,
                e.getMessage() != null ? e.getMessage() : e.getClass().getName());
        }
    }

    /**
     * Plugin self-description.
     *
     * <p>TODO(loader-integration, out of scope for #1402/PR-6): the strict
     * PEP 1.7.0 loader in {@code provekit-plugin-loader} expects a full plugin
     * memento envelope ({@code {envelope, header:{cid, content, ...}, metadata}})
     * and recomputes the {@code header.cid} to verify it. This kit returns a
     * simpler capability summary because PR-6's scope is the emitter + module +
     * tests + PR, NOT substrate-side loader wiring (which also needs a
     * pre-computed PLUGIN_CID over a canonical content payload this kit does
     * not yet mint). Wire the envelope/header/CID when integrating the kit
     * into the loader registry.
     */
    private String describeResult() {
        return "{"
            + "\"name\":\"provekit-emit-java-junit\","
            + "\"version\":\"0.1.0\","
            + "\"protocol_versions\":[\"pep/1.7.0\"],"
            + "\"kind\":\"realize\","
            + "\"target_language\":\"java\","
            + "\"target_framework\":\"junit5\","
            + "\"capabilities\":{"
            + "\"kits\":[\"java\"],"
            + "\"emits\":\"junit5-assertions\","
            + "\"predicates\":["
            + "\"concept:eq\",\"concept:ne\",\"concept:lt\",\"concept:gt\","
            + "\"concept:le\",\"concept:ge\",\"concept:option-is-some\","
            + "\"concept:option-is-none\",\"concept:fallible-err\""
            + "]"
            + "}"
            + "}";
    }

    private void sendResponse(String id, String result) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}");
    }

    private void sendError(String id, int code, String message) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id
            + ",\"error\":{\"code\":" + code + ",\"message\":\"" + escape(message) + "\"}}");
    }

    private static String escape(String s) {
        if (s == null) return "";
        return s.replace("\\", "\\\\").replace("\"", "\\\"")
            .replace("\n", "\\n").replace("\r", "\\r");
    }
}
