// SPDX-License-Identifier: Apache-2.0
//
// JSON-RPC server for the Java bind-lift kit. Speaks PEP 1.7.0
// (`provekit.plugin.invoke`) over stdio. Methods: `initialize`, `lift`,
// `shutdown`. Output shape: `ir-document` of `bind-lift-entry` records per
// `2026-05-13-bind-ir-lift-result.md`.
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
    private static final String SHARED_LSP_PROTOCOL = "provekit-lsp-shared/1";
    private static final String PEP_PROTOCOL = "pep/1.7.0";

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
            case "initialize" -> initialize(id, request.get("params"));
            case "analyzeDocument" -> analyzeDocument(id, request.get("params"));
            case "lift" -> lift(id, request.get("params"));
            case "shutdown" -> Jcs.object("jsonrpc", Jcs.string("2.0"), "id", id, "result", Jcs.nullValue());
            default -> error(id, -32601, "METHOD_NOT_FOUND: " + method);
        };
    }

    private static Jcs.Obj initialize(Jcs.Json id, Jcs.Json paramsJson) {
        if (isSharedLspInitialize(paramsJson)) {
            return Jcs.object(
                "id", id,
                "jsonrpc", Jcs.string("2.0"),
                "result", Jcs.object(
                    "capabilities", Jcs.object(
                        "diagnostic_codes", Jcs.array(
                            Jcs.string("provekit.lsp.parse_error"),
                            Jcs.string("provekit.lsp.lift_gap")
                        ),
                        "entry_kinds", Jcs.array(
                            Jcs.string("bind-lift-entry"),
                            Jcs.string("library-sugar-binding-entry"),
                            Jcs.string("refusal-memento")
                        ),
                        "methods", Jcs.array(
                            Jcs.string("analyzeDocument"),
                            Jcs.string("lift"),
                            Jcs.string("shutdown")
                        ),
                        "status_kinds", Jcs.array(
                            Jcs.string("lift"),
                            Jcs.string("materialize"),
                            Jcs.string("emit"),
                            Jcs.string("check"),
                            Jcs.string("prove")
                        ),
                        "surfaces", Jcs.array(Jcs.string("java"), Jcs.string("java-bind"))
                    ),
                    "kit_id", Jcs.string(KIT_ID),
                    "name", Jcs.string("provekit-lsp-java"),
                    "protocol_version", Jcs.string(SHARED_LSP_PROTOCOL),
                    "protocol_versions", Jcs.array(Jcs.string(SHARED_LSP_PROTOCOL), Jcs.string(PEP_PROTOCOL)),
                    "version", Jcs.string("0.1.0")
                )
            );
        }
        return Jcs.object(
            "id", id,
            "jsonrpc", Jcs.string("2.0"),
            "result", Jcs.object(
                "capabilities", Jcs.object(
                    "authoring_surfaces", Jcs.array(Jcs.string("java"), Jcs.string("java-bind")),
                    "emits_signed_mementos", Jcs.bool(false),
                    "ir_version", Jcs.string("bind-ir/1.0.0"),
                    "methods", Jcs.array(Jcs.string("lift"), Jcs.string("analyzeDocument"), Jcs.string("shutdown"))
                ),
                "name", Jcs.string("provekit-lift-java-bind"),
                "protocol_version", Jcs.string(PEP_PROTOCOL),
                "protocol_versions", Jcs.array(Jcs.string(PEP_PROTOCOL), Jcs.string(SHARED_LSP_PROTOCOL)),
                "version", Jcs.string("0.1.0")
            )
        );
    }

    private static boolean isSharedLspInitialize(Jcs.Json paramsJson) {
        if (!(paramsJson instanceof Jcs.Obj params)) return false;
        return SHARED_LSP_PROTOCOL.equals(params.stringFieldOrNull("protocol_version"));
    }

    private static Jcs.Obj analyzeDocument(Jcs.Json id, Jcs.Json paramsJson) {
        if (!(paramsJson instanceof Jcs.Obj params)) {
            return error(id, -32602, "INVALID_PARAMS: params object required");
        }
        String requestedKit = params.stringFieldOrNull("kit_id");
        if (requestedKit != null && !requestedKit.isBlank() && !KIT_ID.equals(requestedKit)) {
            return error(id, -32602, "INVALID_PARAMS: kit_id must be `java` for the Java LSP helper");
        }
        String text = firstString(params, "text", "source");
        if (text == null) text = "";
        String file = firstString(params, "file", "path");
        if (file == null || file.isBlank()) file = "Document.java";
        String uri = params.stringFieldOrNull("uri");
        if (uri == null || uri.isBlank()) uri = file;

        JavaBindLifter.Result lifted = new JavaBindLifter().liftPathsFromSource(file, text);
        List<Jcs.Json> entries = new ArrayList<>();
        for (Jcs.Json rawEntry : lifted.entries()) {
            if (!(rawEntry instanceof Jcs.Obj entry)) continue;
            String kind = entry.stringFieldOrNull("kind");
            if (kind == null || kind.isBlank()) kind = "unknown-entry";
            entries.add(Jcs.object(
                "entry", entry,
                "file", Jcs.string(file),
                "kind", Jcs.string(kind),
                "kit_id", Jcs.string(KIT_ID),
                "producer", Jcs.string("kit"),
                "range", rangeForEntry(entry)
            ));
        }

        List<Jcs.Json> diagnostics = new ArrayList<>();
        for (Jcs.Json rawDiagnostic : lifted.diagnostics()) {
            if (!(rawDiagnostic instanceof Jcs.Obj diagnostic)) continue;
            diagnostics.add(sharedDiagnostic(file, diagnostic));
        }

        Jcs.Obj result = Jcs.object(
            "diagnostics", Jcs.array(diagnostics),
            "document_cid", Jcs.string(Jcs.blake3_512(text.getBytes(StandardCharsets.UTF_8))),
            "entries", Jcs.array(entries),
            "file", Jcs.string(file),
            "kind", Jcs.string("lsp-document-analysis"),
            "kit_id", Jcs.string(KIT_ID),
            "project", Jcs.nullValue(),
            "protocol_catalog_cid", Jcs.string(protocolCatalogCid()),
            "schema_version", Jcs.string("1"),
            "statuses", Jcs.array(statuses(entries, diagnostics)),
            "uri", Jcs.string(uri)
        );
        return Jcs.object("id", id, "jsonrpc", Jcs.string("2.0"), "result", result);
    }

    private static String firstString(Jcs.Obj params, String first, String second) {
        String value = params.stringFieldOrNull(first);
        if (value != null) return value;
        return params.stringFieldOrNull(second);
    }

    private static Jcs.Obj sharedDiagnostic(String file, Jcs.Obj diagnostic) {
        String rawKind = diagnostic.stringFieldOrNull("kind");
        String message = diagnostic.stringFieldOrNull("message");
        if (message == null) message = "Java lift diagnostic";
        String severity = "error".equals(rawKind) ? "error" : "warning";
        String code = "error".equals(rawKind) ? "provekit.lsp.parse_error" : "provekit.lsp.lift_gap";
        long line = numberField(diagnostic, "line", 1);
        return Jcs.object(
            "code", Jcs.string(code),
            "file", Jcs.string(file),
            "kit_id", Jcs.string(KIT_ID),
            "message", Jcs.string(message),
            "producer", Jcs.string("kit"),
            "range", lineRange(line),
            "severity", Jcs.string(severity),
            "source", Jcs.string("provekit")
        );
    }

    private static List<Jcs.Json> statuses(List<Jcs.Json> entries, List<Jcs.Json> diagnostics) {
        List<Jcs.Json> statuses = new ArrayList<>();
        statuses.add(status(
            "lift",
            diagnostics.isEmpty() ? "available" : "partial",
            diagnostics.isEmpty()
                ? "Java source lifted through the Java bind lifter"
                : "Java source lifted with kit diagnostics"
        ));
        statuses.add(status(
            "materialize",
            "unknown",
            "Java materialize availability is kit-owned and not yet queried by this LSP helper"
        ));
        statuses.add(status(
            "emit",
            "unknown",
            "Java JUnit emitter availability is kit-owned and not yet queried by this LSP helper"
        ));
        statuses.add(status(
            "check",
            "unknown",
            "Java compile/test check status must come from the Java kit, not the coordinator"
        ));
        statuses.add(status(
            "prove",
            "unknown",
            "Proof verdicts are verifier/coordinator data; the kit does not report vacuous success"
        ));
        return statuses;
    }

    private static Jcs.Obj status(String kind, String status, String message) {
        return Jcs.object(
            "kind", Jcs.string(kind),
            "kit_id", Jcs.string(KIT_ID),
            "message", Jcs.string(message),
            "producer", Jcs.string("kit"),
            "status", Jcs.string(status)
        );
    }

    private static Jcs.Obj rangeForEntry(Jcs.Obj entry) {
        Jcs.Json bodySourceJson = entry.get("body_source");
        if (bodySourceJson instanceof Jcs.Obj bodySource) {
            Jcs.Json spanJson = bodySource.get("span");
            if (spanJson instanceof Jcs.Obj span) {
                return Jcs.object(
                    "end_col", Jcs.integer(numberField(span, "end_col", 1)),
                    "end_line", Jcs.integer(numberField(span, "end_line", numberField(span, "start_line", 1))),
                    "start_col", Jcs.integer(numberField(span, "start_col", 0)),
                    "start_line", Jcs.integer(numberField(span, "start_line", 1))
                );
            }
        }
        return lineRange(numberField(entry, "fn_line", 1));
    }

    private static Jcs.Obj lineRange(long line) {
        long safeLine = Math.max(1, line);
        return Jcs.object(
            "end_col", Jcs.integer(1),
            "end_line", Jcs.integer(safeLine),
            "start_col", Jcs.integer(0),
            "start_line", Jcs.integer(safeLine)
        );
    }

    private static long numberField(Jcs.Obj obj, String key, long fallback) {
        Jcs.Json value = obj.get(key);
        if (value instanceof Jcs.Num n) return n.value();
        return fallback;
    }

    private static String protocolCatalogCid() {
        return Jcs.blake3_512((SHARED_LSP_PROTOCOL + ":" + KIT_ID).getBytes(StandardCharsets.UTF_8));
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
