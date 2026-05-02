package com.provekit.lift;

import java.io.*;
import java.nio.file.*;
import java.util.*;
import com.github.javaparser.*;
import com.github.javaparser.ast.*;

public class RpcServer {
    private final BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    private final PrintWriter out = new PrintWriter(System.out, true);
    private final LiftHandler liftHandler = new LiftHandler();

    public void run() {
        try {
            String line;
            while ((line = in.readLine()) != null) {
                handle(line.trim());
            }
        } catch (IOException e) {
            System.err.println("RPC read error: " + e.getMessage());
        }
    }

    private void handle(String line) {
        if (line.isEmpty()) return;
        try {
            String id = extractId(line);
            String method = extractMethod(line);
            switch (method) {
                case "initialize" -> sendResponse(id, initResult());
                case "lift" -> {
                    String workspace = extractStringField(line, "workspace_root");
                    String surface = extractStringField(line, "surface");
                    sendResponse(id, liftHandler.lift(workspace, surface));
                }
                case "shutdown" -> {
                    sendResponse(id, "null");
                    System.exit(0);
                }
                default -> sendError(id, -32601, "unknown method: " + method);
            }
        } catch (Exception e) {
            System.err.println("Handler error: " + e.getMessage());
        }
    }

    private String initResult() {
        return "{\"name\":\"provekit-lift-java\",\"version\":\"0.1.0\",\"protocol_version\":\"provekit-lift/1\",\"capabilities\":{\"authoring_surfaces\":[\"java-bean-validation\",\"java-jml\",\"java-cofoja\"],\"ir_version\":\"v1.1.0\",\"emits_signed_mementos\":false}}";
    }

    private void sendResponse(String id, String result) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}");
    }

    private void sendError(String id, int code, String message) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"error\":{\"code\":" + code + ",\"message\":\"" + escape(message) + "\"}}");
    }

    static String extractId(String json) {
        int i = json.indexOf("\"id\"");
        if (i < 0) return "null";
        int colon = json.indexOf(':', i + 4);
        int comma = json.indexOf(',', colon);
        int brace = json.indexOf('}', colon);
        int end = (comma >= 0 && comma < brace) ? comma : brace;
        return json.substring(colon + 1, end).trim();
    }

    static String extractMethod(String json) {
        int i = json.indexOf("\"method\"");
        if (i < 0) return "";
        int q1 = json.indexOf('"', i + 8);
        int q2 = json.indexOf('"', q1 + 1);
        return json.substring(q1 + 1, q2);
    }

    static String extractStringField(String json, String field) {
        String key = "\"" + field + "\"";
        int i = json.indexOf(key);
        if (i < 0) return ".";
        int q1 = json.indexOf('"', i + key.length());
        if (q1 < 0) return ".";
        int q2 = json.indexOf('"', q1 + 1);
        return json.substring(q1 + 1, q2);
    }

    static String escape(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
    }
}
