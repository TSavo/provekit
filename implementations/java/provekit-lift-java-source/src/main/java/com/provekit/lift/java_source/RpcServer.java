package com.provekit.lift.java_source;

import com.provekit.ir.Jcs;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.util.List;

public final class RpcServer {
    public static final String Surface = "java";
    public static final String Dialect = "java-source";
    public static final String Version = "0.1.0-draft";

    private RpcServer() {}

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
            case "compile" -> compile(id, request.get("params"));
            case "shutdown" -> Jcs.object("jsonrpc", Jcs.string("2.0"), "id", id, "result", Jcs.nullValue());
            default -> error(id, -32601, "METHOD_NOT_FOUND: " + method);
        };
    }

    private static Jcs.Obj initialize(Jcs.Json id) {
        return Jcs.object(
            "jsonrpc", Jcs.string("2.0"),
            "id", id,
            "result", Jcs.object(
                "name", Jcs.string("provekit-lift-java-source"),
                "version", Jcs.string(Version),
                "protocol_version", Jcs.string("provekit-lift/1"),
                "capabilities", Jcs.object(
                    "authoring_surfaces", Jcs.array(Jcs.string(Dialect)),
                    "dialect", Jcs.string(Dialect),
                    "ir_version", Jcs.string("v1.1.0"),
                    "emits_signed_mementos", Jcs.bool(false)
                )
            )
        );
    }

    private static Jcs.Obj lift(Jcs.Json id, Jcs.Json paramsJson) {
        if (!(paramsJson instanceof Jcs.Obj params)) return error(id, -32602, "params required");
        String surface = params.stringFieldOrNull("surface");
        if (surface == null) surface = Dialect;
        if (!surface.equals(Surface) && !surface.equals(Dialect)) return error(id, 1003, "SURFACE_NOT_SUPPORTED: " + surface);
        Jcs.Json pathsJson = params.get("source_paths");
        if (!(pathsJson instanceof Jcs.Arr pathsArr) || pathsArr.isEmpty()) return error(id, -32602, "source_paths must be a non-empty array");
        List<String> paths = pathsArr.values().stream().map(v -> ((Jcs.Str) v).value()).toList();
        String root = params.stringFieldOrNull("workspace_root");
        if (root == null) root = ".";
        JavaSourceLifter.LiftResult result = new JavaSourceLifter().liftPaths(root, paths);
        return Jcs.object("jsonrpc", Jcs.string("2.0"), "id", id, "result", result.toJson());
    }

    private static Jcs.Obj compile(Jcs.Json id, Jcs.Json paramsJson) {
        if (!(paramsJson instanceof Jcs.Obj params)) return error(id, -32602, "params required");
        Jcs.Json ir = params.get("ir");
        if (ir == null) return error(id, -32602, "ir required");
        String body = new JavaSourceCompiler().compile(ir);
        return Jcs.object(
            "jsonrpc", Jcs.string("2.0"),
            "id", id,
            "result", Jcs.object("kind", Jcs.string("compiled-formula"), "body", Jcs.string(body))
        );
    }

    private static Jcs.Obj error(Jcs.Json id, int code, String message) {
        return Jcs.object(
            "jsonrpc", Jcs.string("2.0"),
            "id", id,
            "error", Jcs.object("code", Jcs.integer(code), "message", Jcs.string(message))
        );
    }
}
