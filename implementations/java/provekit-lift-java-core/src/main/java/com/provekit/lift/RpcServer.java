package com.provekit.lift;

import java.io.*;
import java.nio.file.*;
import java.util.*;
import com.github.javaparser.*;
import com.github.javaparser.ast.*;
import com.provekit.ir.Jcs;

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
                case "parse" -> {
                    // Daemon-conformant wire protocol: {path, source} -> {declarations, callEdges, warnings}.
                    // Uses decodeJsonString to correctly handle source code with embedded quotes/escapes.
                    String path = decodeJsonStringField(line, "path");
                    String source = decodeJsonStringField(line, "source");
                    sendResponse(id, liftHandler.parseSource(path, source));
                }
                case "lift" -> {
                    // Legacy CLI protocol: {workspace_root, surface}.
                    // workspace_root and surface are short paths/identifiers without embedded quotes.
                    String workspace = extractStringField(line, "workspace_root");
                    String surface = extractStringField(line, "surface");
                    String emitMode = extractEmitMode(line);
                    sendResponse(id, liftHandler.lift(workspace, surface, emitMode));
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
        return "{\"name\":\"provekit-lsp-java\",\"version\":\"0.1.0\",\"capabilities\":[\"parse\"]}";
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

    /**
     * Extract and decode a JSON string field value from a flat JSON object.
     *
     * This is the escape-aware counterpart to extractStringField. It correctly
     * handles escaped quotes, escaped backslashes, newlines, carriage returns,
     * tabs, and Unicode escape sequences within the value. Required for the
     * 'source' field which may contain arbitrary Java source code.
     *
     * Returns "" if the field is not found.
     */
    static String decodeJsonStringField(String json, String field) {
        String key = "\"" + field + "\"";
        int ki = json.indexOf(key);
        if (ki < 0) return "";

        // Skip past the key, colon, optional whitespace, then opening quote.
        int pos = ki + key.length();
        while (pos < json.length() && (json.charAt(pos) == ':' || json.charAt(pos) == ' ' || json.charAt(pos) == '\t')) {
            pos++;
        }
        if (pos >= json.length() || json.charAt(pos) != '"') return "";
        pos++; // skip opening quote

        StringBuilder sb = new StringBuilder();
        while (pos < json.length()) {
            char c = json.charAt(pos);
            if (c == '"') {
                // Closing quote: done.
                break;
            } else if (c == '\\') {
                pos++;
                if (pos >= json.length()) break;
                char esc = json.charAt(pos);
                switch (esc) {
                    case '"'  -> sb.append('"');
                    case '\\' -> sb.append('\\');
                    case '/'  -> sb.append('/');
                    case 'n'  -> sb.append('\n');
                    case 'r'  -> sb.append('\r');
                    case 't'  -> sb.append('\t');
                    case 'b'  -> sb.append('\b');
                    case 'f'  -> sb.append('\f');
                    case 'u'  -> {
                        if (pos + 4 < json.length()) {
                            String hex = json.substring(pos + 1, pos + 5);
                            try {
                                sb.append((char) Integer.parseInt(hex, 16));
                                pos += 4;
                            } catch (NumberFormatException e) {
                                sb.append('u'); // best-effort: emit raw
                            }
                        }
                    }
                    default -> sb.append(esc);
                }
            } else {
                sb.append(c);
            }
            pos++;
        }
        return sb.toString();
    }

    /**
     * Legacy field extractor: terminates at the first closing quote.
     * Safe for short fields like workspace_root and surface that never contain escapes.
     */
    static String extractStringField(String json, String field) {
        String key = "\"" + field + "\"";
        int i = json.indexOf(key);
        if (i < 0) return ".";
        int q1 = json.indexOf('"', i + key.length());
        if (q1 < 0) return ".";
        int q2 = json.indexOf('"', q1 + 1);
        return json.substring(q1 + 1, q2);
    }

    static String extractEmitMode(String json) {
        try {
            Jcs.Json doc = Jcs.parse(json);
            if (doc instanceof Jcs.Obj obj
                    && obj.get("params") instanceof Jcs.Obj params
                    && params.get("options") instanceof Jcs.Obj options) {
                String emit = options.stringFieldOrNull("emit");
                if (emit != null && !emit.isBlank()) {
                    return emit;
                }
            }
        } catch (RuntimeException ignored) {
            // Fall through to the protocol default below; parse errors are handled elsewhere.
        }
        return "proof-envelope";
    }

    static String escape(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
    }
}
