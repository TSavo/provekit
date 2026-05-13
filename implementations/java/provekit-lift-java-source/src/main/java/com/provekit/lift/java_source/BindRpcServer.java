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
import java.util.ArrayList;
import java.util.List;

public final class BindRpcServer {
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
                    "authoring_surfaces", Jcs.array(Jcs.string("java"), Jcs.string("java-bind")),
                    "emits_signed_mementos", Jcs.bool(false),
                    "ir_version", Jcs.string("bind-ir/1.0.0")
                ),
                "name", Jcs.string("provekit-lift-java-bind"),
                "protocol_version", Jcs.string("pep/1.7.0"),
                "version", Jcs.string("0.1.0")
            )
        );
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
