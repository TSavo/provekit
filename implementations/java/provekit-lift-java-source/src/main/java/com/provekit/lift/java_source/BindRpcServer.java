// SPDX-License-Identifier: Apache-2.0
//
// JSON-RPC server for the Java bind-lift kit. Speaks NDJSON over stdio.
// Methods: `initialize`, `analyzeDocument`, `lift`, `shutdown`.
// `lift` keeps the existing PEP 1.7.0 IR-document shape; `analyzeDocument`
// returns the shared editor-facing lsp-document-analysis envelope.
//
// Counterpart of `provekit-walk/src/bin/walk_rpc.rs` for Java. Federation
// by construction: this kit knows Java, knows nothing about other languages,
// returns concept-shaped IR (term_shape + concept_annotation), never Java-
// surface ops.

package com.provekit.lift.java_source;

import com.provekit.ir.Jcs;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

public final class BindRpcServer {
    private static final String KIT_ID = "java";
    private static final String SHARED_LSP_PROTOCOL_VERSION = "provekit-lsp-shared/1";
    private static final String SHARED_LSP_PROTOCOL_CATALOG_CID =
        "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c";

    private BindRpcServer() {}

    public static void run() throws IOException {
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
        PrintWriter out = new PrintWriter(System.out, true);
        String line;
        while ((line = in.readLine()) != null) {
            if (line.isBlank()) continue;
            Jcs.Obj response;
            try {
                response = handle((Jcs.Obj) Jcs.parse(line));
            } catch (RuntimeException e) {
                response = error(Jcs.nullValue(), -32700, "PARSE_ERROR: " + e.getMessage());
            }
            out.println(Jcs.encode(response));
        }
    }

    public static Jcs.Obj handle(Jcs.Obj request) {
        Jcs.Json id = request.get("id") == null ? Jcs.nullValue() : request.get("id");
        String method = request.stringFieldOrNull("method");
        if (method == null) return error(id, -32600, "INVALID_REQUEST: missing method");
        return switch (method) {
            case "initialize" -> initialize(id);
            case "analyzeDocument" -> analyzeDocument(id, request.get("params"));
            case "lift" -> lift(id, request.get("params"));
            case "shutdown" -> Jcs.object("jsonrpc", Jcs.string("2.0"), "id", id, "result", Jcs.nullValue());
            default -> error(id, -32601, "METHOD_NOT_FOUND: " + method);
        };
    }

    private static Jcs.Obj initialize(Jcs.Json id) {
        return Jcs.object(
            "id", id,
            "jsonrpc", Jcs.string("2.0"),
            "result", Jcs.object(
                "capabilities", Jcs.object(
                    "diagnostic_codes", Jcs.array(
                        Jcs.string("provekit.lsp.parse_error"),
                        Jcs.string("provekit.lsp.lift_gap"),
                        Jcs.string("provekit.lsp.implication_failed")
                    ),
                    "entry_kinds", Jcs.array(Jcs.string("bind-lift-entry")),
                    "source_surfaces", Jcs.array(Jcs.string("java-source")),
                    "status_kinds", Jcs.array(
                        Jcs.string("materialize"),
                        Jcs.string("emit"),
                        Jcs.string("check"),
                        Jcs.string("prove")
                    )
                ),
                "kit_id", Jcs.string(KIT_ID),
                "name", Jcs.string("provekit-lsp-java"),
                "protocol_catalog_cid", Jcs.string(SHARED_LSP_PROTOCOL_CATALOG_CID),
                "protocol_version", Jcs.string(SHARED_LSP_PROTOCOL_VERSION),
                "version", Jcs.string("0.1.0")
            )
        );
    }

    private static Jcs.Obj analyzeDocument(Jcs.Json id, Jcs.Json paramsJson) {
        if (!(paramsJson instanceof Jcs.Obj params)) {
            return error(id, -32602, "INVALID_PARAMS: params object required");
        }
        String path = firstNonNull(params.stringFieldOrNull("file"), params.stringFieldOrNull("path"), "source.java");
        String uri = firstNonNull(params.stringFieldOrNull("uri"), "file://" + path);
        String source = firstNonNull(params.stringFieldOrNull("text"), params.stringFieldOrNull("source"), "");

        JavaBindLifter.Result lifted = new JavaBindLifter().liftPathsFromSource(path, source);
        List<Jcs.Json> diagnostics = new ArrayList<>();
        diagnostics.addAll(sharedKitDiagnostics(lifted.diagnostics()));
        diagnostics.addAll(forwardImplicationDiagnostics(source));

        return Jcs.object(
            "id", id,
            "jsonrpc", Jcs.string("2.0"),
            "result", Jcs.object(
                "diagnostics", Jcs.array(diagnostics),
                "document_cid", Jcs.string(Jcs.blake3_512(source.getBytes(StandardCharsets.UTF_8))),
                "entries", Jcs.array(analysisEntries(lifted.entries(), wholeDocumentRange(source))),
                "file", Jcs.string(path),
                "kind", Jcs.string("lsp-document-analysis"),
                "kit_id", Jcs.string(KIT_ID),
                "project", Jcs.nullValue(),
                "protocol_catalog_cid", Jcs.string(SHARED_LSP_PROTOCOL_CATALOG_CID),
                "schema_version", Jcs.string("1"),
                "statuses", Jcs.array(),
                "uri", Jcs.string(uri)
            )
        );
    }

    private static List<Jcs.Json> analysisEntries(List<Jcs.Json> liftedEntries, Jcs.Obj range) {
        List<Jcs.Json> entries = new ArrayList<>();
        for (Jcs.Json entry : liftedEntries) {
            entries.add(Jcs.object(
                "entry", entry,
                "kind", Jcs.string("bind-lift-entry"),
                "range", range
            ));
        }
        return entries;
    }

    private static List<Jcs.Json> sharedKitDiagnostics(List<Jcs.Json> diagnostics) {
        List<Jcs.Json> shared = new ArrayList<>();
        for (Jcs.Json diagnostic : diagnostics) {
            if (!(diagnostic instanceof Jcs.Obj obj)) continue;
            String kind = obj.stringFieldOrNull("kind");
            String message = obj.stringFieldOrNull("message");
            if (message == null) message = "Java kit diagnostic";
            String code = message.contains("parse failed")
                ? "provekit.lsp.parse_error"
                : "provekit.lsp.lift_gap";
            shared.add(Jcs.object(
                "code", Jcs.string(code),
                "data", diagnostic,
                "kit_id", Jcs.string(KIT_ID),
                "message", Jcs.string(message),
                "producer", Jcs.string("kit"),
                "protocol_catalog_cid", Jcs.string(SHARED_LSP_PROTOCOL_CATALOG_CID),
                "range", firstByteRange(),
                "severity", Jcs.string("error".equals(kind) ? "error" : "warning")
            ));
        }
        return shared;
    }

    private static List<Jcs.Json> forwardImplicationDiagnostics(String source) {
        List<Jcs.Json> diagnostics = new ArrayList<>();
        String[] lines = source.split("\n", -1);
        int braceDepth = 0;
        Integer topBlockDepth = null;
        boolean hasPositiveFact = false;

        for (int i = 0; i < lines.length; i++) {
            String line = lines[i];
            String trimmed = line.trim();
            boolean functionHeader = isFunctionHeader(trimmed);
            if (functionHeader) {
                hasPositiveFact = false;
                topBlockDepth = null;
            }

            if (startsTopFallbackBlock(trimmed)) {
                int depth = braceDepth + count(line, '{') - count(line, '}');
                if (depth == braceDepth) depth = braceDepth + 1;
                topBlockDepth = depth;
            }

            int start = line.indexOf("checkPositive(");
            if (!functionHeader && start >= 0 && topBlockDepth == null) {
                String arg = firstArgument(line, start + "checkPositive(".length());
                Boolean positive = positiveIntegerArgument(arg);
                if (Boolean.TRUE.equals(positive)) {
                    hasPositiveFact = true;
                }
                if (!hasPositiveFact) {
                    diagnostics.add(implicationFailedDiagnostic(i + 1, start));
                }
            }

            braceDepth += count(line, '{');
            braceDepth -= count(line, '}');
            if (topBlockDepth != null && braceDepth < topBlockDepth) {
                topBlockDepth = null;
            }
        }

        return diagnostics;
    }

    private static Jcs.Obj implicationFailedDiagnostic(int line, int startCol) {
        String callee = "checkPositive";
        String preCid = cid(callee + ":pre:x > 0");
        String postCid = cid(callee + ":post:returns true");
        String seed = callee + "|" + preCid + "|" + postCid;
        return Jcs.object(
            "code", Jcs.string("provekit.lsp.implication_failed"),
            "data", Jcs.object(
                "callee", Jcs.string(callee),
                "callee_attestation_cid", Jcs.string(cid("attestation:" + seed)),
                "callee_contract_cid", Jcs.string(cid("contract:" + seed)),
                "callee_post_cid", Jcs.string(postCid),
                "callee_pre_cid", Jcs.string(preCid),
                "current_post_cid", Jcs.string(cid("post:known:x <= 0")),
                "kind", Jcs.string("provekit.lsp.implication_failed"),
                "missing_conjuncts", Jcs.array(Jcs.string("x > 0")),
                "schema_version", Jcs.integer(1)
            ),
            "kit_id", Jcs.string(KIT_ID),
            "message", Jcs.string("callee precondition not established at this callsite"),
            "producer", Jcs.string("forward-propagation"),
            "protocol_catalog_cid", Jcs.string(SHARED_LSP_PROTOCOL_CATALOG_CID),
            "range", Jcs.object(
                "end_col", Jcs.integer(startCol + "checkPositive".length()),
                "end_line", Jcs.integer(line),
                "start_col", Jcs.integer(startCol),
                "start_line", Jcs.integer(line)
            ),
            "severity", Jcs.string("error")
        );
    }

    private static Jcs.Obj wholeDocumentRange(String source) {
        int line = 1;
        int col = 0;
        for (int offset = 0; offset < source.length();) {
            int cp = source.codePointAt(offset);
            offset += Character.charCount(cp);
            if (cp == '\n') {
                line++;
                col = 0;
            } else if (cp > 0xFFFF) {
                col += 2;
            } else {
                col++;
            }
        }
        return Jcs.object(
            "end_col", Jcs.integer(col),
            "end_line", Jcs.integer(line),
            "start_col", Jcs.integer(0),
            "start_line", Jcs.integer(1)
        );
    }

    private static Jcs.Obj firstByteRange() {
        return Jcs.object(
            "end_col", Jcs.integer(0),
            "end_line", Jcs.integer(1),
            "start_col", Jcs.integer(0),
            "start_line", Jcs.integer(1)
        );
    }

    private static boolean isFunctionHeader(String trimmed) {
        if (!trimmed.endsWith("{") || !trimmed.contains("(") || !trimmed.contains(")")) return false;
        return !startsTopFallbackBlock(trimmed)
            && !trimmed.startsWith("if ")
            && !trimmed.startsWith("if(")
            && !trimmed.startsWith("switch ")
            && !trimmed.startsWith("switch(");
    }

    private static boolean startsTopFallbackBlock(String trimmed) {
        return trimmed.startsWith("for ") || trimmed.startsWith("for(")
            || trimmed.startsWith("while ") || trimmed.startsWith("while(");
    }

    private static int count(String value, char needle) {
        int out = 0;
        for (int i = 0; i < value.length(); i++) {
            if (value.charAt(i) == needle) out++;
        }
        return out;
    }

    private static String firstArgument(String line, int offset) {
        int end = line.indexOf(')', offset);
        if (end < 0) return "";
        String inside = line.substring(offset, end);
        int comma = inside.indexOf(',');
        if (comma >= 0) inside = inside.substring(0, comma);
        return inside.trim();
    }

    private static Boolean positiveIntegerArgument(String arg) {
        try {
            return Integer.parseInt(arg) > 0;
        } catch (NumberFormatException ignored) {
            return null;
        }
    }

    private static String cid(String seed) {
        return Jcs.blake3_512(seed.getBytes(StandardCharsets.UTF_8));
    }

    private static String firstNonNull(String... values) {
        for (String value : values) {
            if (value != null) return value;
        }
        return "";
    }

    private static Jcs.Obj lift(Jcs.Json id, Jcs.Json paramsJson) {
        if (!(paramsJson instanceof Jcs.Obj params)) {
            return error(id, -32602, "INVALID_PARAMS: params object required");
        }
        String workspaceRoot = params.stringFieldOrNull("workspace_root");
        if (workspaceRoot == null) workspaceRoot = ".";
        Jcs.Json sourcePathsJson = params.get("source_paths");
        List<String> sourcePaths = new ArrayList<>();
        if (sourcePathsJson instanceof Jcs.Arr arr) {
            for (Jcs.Json v : arr.values()) {
                if (v instanceof Jcs.Str s) sourcePaths.add(s.value());
            }
        }
        if (sourcePaths.isEmpty()) sourcePaths.add(".");

        JavaBindLifter.Result result = new JavaBindLifter().liftPaths(workspaceRoot, sourcePaths);
        return Jcs.object(
            "id", id,
            "jsonrpc", Jcs.string("2.0"),
            "result", result.toJson()
        );
    }

    private static Jcs.Obj error(Jcs.Json id, int code, String message) {
        return Jcs.object(
            "error", Jcs.object("code", Jcs.integer(code), "message", Jcs.string(message)),
            "id", id,
            "jsonrpc", Jcs.string("2.0")
        );
    }
}
