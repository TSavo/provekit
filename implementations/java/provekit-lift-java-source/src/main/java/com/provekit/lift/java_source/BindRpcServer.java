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
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

public final class BindRpcServer {
    private static final String KIT_ID = "java";
    private static final String SHARED_LSP_PROTOCOL = "provekit-lsp-shared/1";
    private static final String LSP_PROTOCOL_CATALOG_REPO_PATH =
        "protocol/catalogs/provekit-lsp-shared-1.catalog.json";
    private static final String EXPECTED_LSP_PROTOCOL_CATALOG_CID =
        "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c";
    private static final String PEP_PROTOCOL = "pep/1.7.0";
    private static volatile String cachedProtocolCatalogCid;

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
                            Jcs.string("provekit.lsp.lift_gap"),
                            Jcs.string("provekit.lsp.catalog_mismatch"),
                            Jcs.string("provekit.lsp.materialize_unavailable"),
                            Jcs.string("provekit.lsp.materialize_refused"),
                            Jcs.string("provekit.lsp.emit_unavailable"),
                            Jcs.string("provekit.lsp.check_failed"),
                            Jcs.string("provekit.lsp.unresolved_symbol"),
                            Jcs.string("provekit.lsp.unprovable_obligation"),
                            Jcs.string("provekit.lsp.implication_failed"),
                            Jcs.string("provekit.lsp.vacuous_proof")
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
                        "source_surfaces", Jcs.array(Jcs.string("java"), Jcs.string("java-bind")),
                        "surfaces", Jcs.array(Jcs.string("java"), Jcs.string("java-bind"))
                    ),
                    "kit_id", Jcs.string(KIT_ID),
                    "name", Jcs.string("provekit-lsp-java"),
                    "protocol_catalog_cid", Jcs.string(protocolCatalogCid()),
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
        Jcs.Obj primaryRange = null;
        for (Jcs.Json rawEntry : lifted.entries()) {
            if (!(rawEntry instanceof Jcs.Obj entry)) continue;
            String kind = entry.stringFieldOrNull("kind");
            if (kind == null || kind.isBlank()) kind = "unknown-entry";
            Jcs.Obj range = rangeForEntry(entry, text);
            if (primaryRange == null) primaryRange = range;
            entries.add(Jcs.object(
                "entry", entry,
                "file", Jcs.string(file),
                "kind", Jcs.string(kind),
                "kit_id", Jcs.string(KIT_ID),
                "producer", Jcs.string("kit"),
                "range", range
            ));
        }
        if (primaryRange == null) primaryRange = wholeDocumentRange(text);

        List<Jcs.Json> diagnostics = new ArrayList<>();
        for (Jcs.Json rawDiagnostic : lifted.diagnostics()) {
            if (!(rawDiagnostic instanceof Jcs.Obj diagnostic)) continue;
            diagnostics.add(sharedDiagnostic(file, diagnostic, text));
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
            "statuses", Jcs.array(statuses(entries, diagnostics, primaryRange)),
            "uri", Jcs.string(uri)
        );
        return Jcs.object("id", id, "jsonrpc", Jcs.string("2.0"), "result", result);
    }

    private static String firstString(Jcs.Obj params, String first, String second) {
        String value = params.stringFieldOrNull(first);
        if (value != null) return value;
        return params.stringFieldOrNull(second);
    }

    private static Jcs.Obj sharedDiagnostic(String file, Jcs.Obj diagnostic, String source) {
        String rawKind = diagnostic.stringFieldOrNull("kind");
        String rawCode = diagnostic.stringFieldOrNull("code");
        String message = diagnostic.stringFieldOrNull("message");
        if (message == null) message = "Java lift diagnostic";
        String severity = "error".equals(rawKind) ? "error" : "warning";
        String code = normalizeDiagnosticCode(rawCode == null ? rawKind : rawCode);
        long line = numberField(diagnostic, "line", 1);
        long col = numberField(diagnostic, "col", 0);
        return Jcs.object(
            "code", Jcs.string(code),
            "file", Jcs.string(file),
            "kit_id", Jcs.string(KIT_ID),
            "message", Jcs.string(message),
            "producer", Jcs.string("kit"),
            "protocol_catalog_cid", Jcs.string(protocolCatalogCid()),
            "range", lineRange(source, line, col),
            "severity", Jcs.string(severity),
            "source", Jcs.string("provekit")
        );
    }

    private static String normalizeDiagnosticCode(String rawCode) {
        if (rawCode != null && rawCode.startsWith("provekit.lsp.")) return rawCode;
        String key = rawCode == null ? "" : rawCode.toLowerCase(Locale.ROOT).replace('-', '_');
        return switch (key) {
            case "error", "parse_error" -> "provekit.lsp.parse_error";
            case "catalog_mismatch" -> "provekit.lsp.catalog_mismatch";
            case "materialize_unavailable" -> "provekit.lsp.materialize_unavailable";
            case "materialize_refused" -> "provekit.lsp.materialize_refused";
            case "emit_unavailable" -> "provekit.lsp.emit_unavailable";
            case "check_failed" -> "provekit.lsp.check_failed";
            case "unresolved_symbol" -> "provekit.lsp.unresolved_symbol";
            case "unprovable_obligation" -> "provekit.lsp.unprovable_obligation";
            case "implication_failed" -> "provekit.lsp.implication_failed";
            case "vacuous_proof" -> "provekit.lsp.vacuous_proof";
            case "warning", "lift_gap" -> "provekit.lsp.lift_gap";
            default -> "provekit.lsp.lift_gap";
        };
    }

    private static List<Jcs.Json> statuses(List<Jcs.Json> entries, List<Jcs.Json> diagnostics, Jcs.Obj range) {
        List<Jcs.Json> statuses = new ArrayList<>();
        statuses.add(status(
            "lift",
            entries.isEmpty() ? "unavailable" : "available",
            diagnostics.isEmpty()
                ? "Java source lifted through the Java bind lifter"
                : "Java source lifted with kit diagnostics",
            range
        ));
        statuses.add(status(
            "materialize",
            "unknown",
            "Java materialize availability is kit-owned and not yet queried by this LSP helper",
            range
        ));
        statuses.add(status(
            "emit",
            "unknown",
            "Java JUnit emitter availability is kit-owned and not yet queried by this LSP helper",
            range
        ));
        statuses.add(status(
            "check",
            "unknown",
            "Java compile/test check status must come from the Java kit, not the coordinator",
            range
        ));
        statuses.add(status(
            "prove",
            "unknown",
            "Proof verdicts are verifier/coordinator data; the kit does not report vacuous success",
            range
        ));
        return statuses;
    }

    private static Jcs.Obj status(String kind, String state, String message, Jcs.Obj range) {
        return Jcs.object(
            "kind", Jcs.string(kind),
            "kit_id", Jcs.string(KIT_ID),
            "message", Jcs.string(message),
            "producer", Jcs.string("kit"),
            "range", range,
            "state", Jcs.string(state)
        );
    }

    private static Jcs.Obj rangeForEntry(Jcs.Obj entry, String source) {
        Jcs.Json sourceRangeJson = entry.get("source_range");
        if (sourceRangeJson instanceof Jcs.Obj sourceRange) {
            return sourceRange(sourceRange, source);
        }
        Jcs.Json bodySourceJson = entry.get("body_source");
        if (bodySourceJson instanceof Jcs.Obj bodySource) {
            Jcs.Json spanJson = bodySource.get("span");
            if (spanJson instanceof Jcs.Obj span) {
                return sourceRange(span, source);
            }
        }
        return lineRange(source, numberField(entry, "fn_line", 1), 0);
    }

    private static Jcs.Obj sourceRange(Jcs.Obj span, String source) {
        long startLine = Math.max(1, numberField(span, "start_line", 1));
        long startCol = Math.max(0, numberField(span, "start_col", 0));
        long endLine = Math.max(startLine, numberField(span, "end_line", startLine));
        long endCol = Math.max(0, numberField(span, "end_col", lineLength(source, endLine)));
        if (endLine == startLine && endCol < startCol) endCol = startCol;
        return Jcs.object(
            "end_col", Jcs.integer(endCol),
            "end_line", Jcs.integer(endLine),
            "start_col", Jcs.integer(startCol),
            "start_line", Jcs.integer(startLine)
        );
    }

    private static Jcs.Obj lineRange(String source, long line, long col) {
        long safeLine = Math.max(1, line);
        long startCol = Math.max(0, col);
        long endCol = Math.max(startCol, lineLength(source, safeLine));
        return Jcs.object(
            "end_col", Jcs.integer(endCol),
            "end_line", Jcs.integer(safeLine),
            "start_col", Jcs.integer(startCol),
            "start_line", Jcs.integer(safeLine)
        );
    }

    private static Jcs.Obj wholeDocumentRange(String source) {
        long endLine = 1;
        long endCol = 0;
        for (int i = 0; i < source.length(); i++) {
            if (source.charAt(i) == '\n') {
                endLine++;
                endCol = 0;
            } else {
                endCol++;
            }
        }
        return Jcs.object(
            "end_col", Jcs.integer(endCol),
            "end_line", Jcs.integer(endLine),
            "start_col", Jcs.integer(0),
            "start_line", Jcs.integer(1)
        );
    }

    private static long lineLength(String source, long oneBasedLine) {
        long currentLine = 1;
        long length = 0;
        for (int i = 0; i < source.length(); i++) {
            char ch = source.charAt(i);
            if (currentLine == oneBasedLine) {
                if (ch == '\n') return length;
                length++;
            } else if (ch == '\n') {
                currentLine++;
            }
        }
        return currentLine == oneBasedLine ? length : 0;
    }

    private static long numberField(Jcs.Obj obj, String key, long fallback) {
        Jcs.Json value = obj.get(key);
        if (value instanceof Jcs.Num n) return n.value();
        return fallback;
    }

    private static String protocolCatalogCid() {
        String cached = cachedProtocolCatalogCid;
        if (cached != null) return cached;
        Path catalogPath = protocolCatalogPath();
        try {
            Jcs.Json catalog = Jcs.parse(Files.readString(catalogPath, StandardCharsets.UTF_8));
            String cid = Jcs.blake3_512(Jcs.encodeUtf8(catalog));
            if (!EXPECTED_LSP_PROTOCOL_CATALOG_CID.equals(cid)) {
                throw new IllegalStateException(
                    "shared LSP protocol catalog CID mismatch: expected "
                        + EXPECTED_LSP_PROTOCOL_CATALOG_CID
                        + " but computed "
                        + cid
                        + " from "
                        + catalogPath
                );
            }
            cachedProtocolCatalogCid = cid;
            return cid;
        } catch (IOException | IllegalArgumentException e) {
            throw new IllegalStateException("failed to load shared LSP protocol catalog from " + catalogPath, e);
        }
    }

    private static Path protocolCatalogPath() {
        List<Path> candidates = new ArrayList<>();
        Path cwd = Path.of("").toAbsolutePath().normalize();
        candidates.add(cwd);
        candidates.addAll(parentPaths(cwd));
        try {
            Path codeLocation = Path.of(BindRpcServer.class.getProtectionDomain().getCodeSource().getLocation().toURI())
                .toAbsolutePath()
                .normalize();
            candidates.add(codeLocation);
            candidates.addAll(parentPaths(codeLocation));
        } catch (Exception ignored) {
        }
        for (Path candidate : candidates) {
            Path catalogPath = candidate.resolve(LSP_PROTOCOL_CATALOG_REPO_PATH);
            if (Files.isRegularFile(catalogPath)) return catalogPath;
        }
        throw new IllegalStateException("cannot locate " + LSP_PROTOCOL_CATALOG_REPO_PATH);
    }

    private static List<Path> parentPaths(Path path) {
        List<Path> parents = new ArrayList<>();
        Path cursor = path.getParent();
        while (cursor != null) {
            parents.add(cursor);
            cursor = cursor.getParent();
        }
        return parents;
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
