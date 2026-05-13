// SPDX-License-Identifier: Apache-2.0
//
// lift-plugin protocol RPC handler. Speaks pep/1.7.0 over NDJSON
// on stdio. Persistent daemon: stays up across multiple lift calls,
// only exits on `shutdown`. Mirrors the daemon-lifecycle pattern
// established by the typescript-self-contracts shim (PR #220) and the
// csharp peer (Provekit.SelfContracts/Program.cs RunRpcMode).
//
// Handshake:
//
//   -> initialize
//   <- {name, version, capabilities}
//   -> lift
//   <- {kind:"proof-envelope", filename_cid, contract_set_cid, bytes_base64}
//   -> shutdown
//   <- null   (then process exits)
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
//
// JSON parsing is hand-rolled; the request shape is small and fixed
// (only `id`, `method` are read). Responses are emitted as strict JSON
// strings via a tiny writer that escapes the same characters JCS does
// (so the bytes round-trip through the rust dispatcher's serde_json
// reader unchanged). We never echo arbitrary user content into the
// JSON payload, so quote-escape rules cover every case we emit.

package com.provekit.selfcontracts;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Base64;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public final class Rpc {

    private Rpc() {}

    private static final Pattern ID_FIELD = Pattern.compile(
        "\"id\"\\s*:\\s*(\"[^\"]*\"|\\d+|null|true|false)");
    private static final Pattern METHOD_FIELD = Pattern.compile(
        "\"method\"\\s*:\\s*\"([^\"]*)\"");

    public static void run() throws IOException {
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        // System.out is the RPC channel; route diagnostics to System.err.
        PrintStream out = System.out;

        String line;
        while ((line = in.readLine()) != null) {
            line = line.trim();
            if (line.isEmpty()) continue;

            String idRaw = matchOrNull(ID_FIELD, line);
            String method = matchOrNull(METHOD_FIELD, line);

            if (method == null) {
                writeError(out, idRaw, -32600, "Invalid Request: missing `method`");
                continue;
            }

            switch (method) {
                case "initialize":
                    writeInitializeResponse(out, idRaw);
                    break;

                case "lift":
                    handleLift(out, idRaw);
                    break;

                case "shutdown":
                    writeResultLiteral(out, idRaw, "null");
                    return;

                default:
                    writeError(out, idRaw, -32601, "METHOD_NOT_FOUND: " + method);
                    break;
            }
        }
    }

    private static String matchOrNull(Pattern p, String line) {
        Matcher m = p.matcher(line);
        if (!m.find()) return null;
        return m.group(p == ID_FIELD ? 1 : 1);
    }

    // -----------------------------------------------------------------

    private static void writeInitializeResponse(PrintStream out, String idRaw) {
        // Hand-emit a stable JSON shape; values are all ASCII-safe literals.
        StringBuilder sb = new StringBuilder(256);
        sb.append("{\"jsonrpc\":\"2.0\",\"id\":").append(idLiteral(idRaw))
          .append(",\"result\":{")
          .append("\"name\":\"java-self-contracts\",")
          .append("\"version\":\"1.0.0\",")
          .append("\"protocol_version\":\"pep/1.7.0\",")
          .append("\"capabilities\":{")
          .append("\"authoring_surfaces\":[\"java-self-contracts\"],")
          .append("\"ir_version\":\"v1.1.0\",")
          .append("\"emits_signed_mementos\":true")
          .append("}}}");
        out.println(sb);
        out.flush();
    }

    private static void handleLift(PrintStream out, String idRaw) throws IOException {
        Path tmpDir = Files.createTempDirectory("provekit-java-rpc-");
        try {
            Orchestrator.MintResult r = Orchestrator.mintOneRun(tmpDir);
            String b64 = Base64.getEncoder().encodeToString(r.bytes);

            StringBuilder sb = new StringBuilder(b64.length() + 256);
            sb.append("{\"jsonrpc\":\"2.0\",\"id\":").append(idLiteral(idRaw))
              .append(",\"result\":{")
              .append("\"kind\":\"proof-envelope\",")
              .append("\"filename_cid\":\"").append(escape(r.cid)).append("\",")
              .append("\"contract_set_cid\":\"").append(escape(r.contractSetCid)).append("\",")
              .append("\"bytes_base64\":\"").append(b64).append("\",")
              .append("\"diagnostics\":[]")
              .append("}}");
            out.println(sb);
            out.flush();
        } catch (RuntimeException | IOException ex) {
            writeError(out, idRaw, 1005, "LIFT_FAILED: " + ex.getMessage());
        } finally {
            // Best-effort cleanup; Files.walk is overkill for a tiny temp dir
            // we just wrote to.
            try {
                deleteDir(tmpDir);
            } catch (IOException ignore) {
                // non-fatal; OS will reclaim under /tmp eventually.
            }
        }
    }

    private static void deleteDir(Path dir) throws IOException {
        if (!Files.exists(dir)) return;
        try (java.util.stream.Stream<Path> walk = Files.walk(dir)) {
            walk.sorted(java.util.Comparator.reverseOrder()).forEach(p -> {
                try { Files.deleteIfExists(p); } catch (IOException ignore) {}
            });
        }
    }

    private static void writeError(PrintStream out, String idRaw, int code, String message) {
        StringBuilder sb = new StringBuilder(128);
        sb.append("{\"jsonrpc\":\"2.0\",\"id\":").append(idLiteral(idRaw))
          .append(",\"error\":{")
          .append("\"code\":").append(code).append(',')
          .append("\"message\":\"").append(escape(message)).append("\"")
          .append("}}");
        out.println(sb);
        out.flush();
    }

    private static void writeResultLiteral(PrintStream out, String idRaw, String literal) {
        StringBuilder sb = new StringBuilder(64);
        sb.append("{\"jsonrpc\":\"2.0\",\"id\":").append(idLiteral(idRaw))
          .append(",\"result\":").append(literal).append("}");
        out.println(sb);
        out.flush();
    }

    /** Pass-through if id was a number / string-literal / null/true/false; otherwise null. */
    private static String idLiteral(String raw) {
        if (raw == null) return "null";
        return raw;
    }

    /** Escape only what JSON requires for a string literal we control. */
    private static String escape(String s) {
        if (s == null) return "";
        StringBuilder sb = new StringBuilder(s.length() + 8);
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"') sb.append("\\\"");
            else if (c == '\\') sb.append("\\\\");
            else if (c < 0x20) {
                sb.append("\\u00");
                sb.append(HEX[(c >>> 4) & 0xF]);
                sb.append(HEX[c & 0xF]);
            } else {
                sb.append(c);
            }
        }
        return sb.toString();
    }

    private static final char[] HEX = {
        '0','1','2','3','4','5','6','7','8','9','a','b','c','d','e','f'
    };
}
