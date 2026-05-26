package com.provekit.lift.java_source;

public final class Program {
    private Program() {}

    public static void main(String[] args) throws Exception {
        if (args.length == 1 && ("--rpc".equals(args[0]) || "--bind-rpc".equals(args[0]))) {
            // PEP 1.7.0 bind-lift surface: emits bind-lift-entry records per
            // protocol/specs/2026-05-13-bind-ir-lift-result.md. cmd_bind
            // dispatches Verb 1 (Lift) here when source_lang=java. The kit
            // emits CONCEPTS via term_shape + concept_annotation, not java:*
            // surface ops.
            //
            // The same Java-owned parser now also serves shared LSP
            // `analyzeDocument` over this JSON-RPC loop when initialized with
            // protocol_version = provekit-lsp-shared/1.
            BindRpcServer.run();
            return;
        }
        System.err.println("usage: java -jar provekit-lift-java-source.jar --rpc|--bind-rpc");
        System.exit(1);
    }
}
