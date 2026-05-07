package com.provekit.realize;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;

public final class RpcServer {
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
            sendError(id, -32000, e.getMessage());
        }
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
