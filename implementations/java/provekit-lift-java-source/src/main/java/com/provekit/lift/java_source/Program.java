package com.provekit.lift.java_source;

public final class Program {
    private Program() {}

    enum RpcMode {
        SOURCE,
        BIND
    }

    public static void main(String[] args) throws Exception {
        RpcMode mode = rpcMode(args);
        if (mode == RpcMode.SOURCE) {
            SourceRpcServer.run();
            return;
        }
        if (mode == RpcMode.BIND) {
            // PEP 1.7.0 bind-lift surface: emits bind-lift-entry records per
            // protocol/specs/2026-05-13-bind-ir-lift-result.md. cmd_bind
            // dispatches Verb 1 (Lift) here when source_lang=java. The kit
            // emits CONCEPTS via term_shape + concept_annotation, not java:*
            // surface ops.
            BindRpcServer.run();
            return;
        }
        System.err.println("usage: java -jar provekit-lift-java-source.jar --rpc|--bind-rpc");
        System.exit(1);
    }

    static RpcMode rpcMode(String[] args) {
        if (args.length != 1) return null;
        return switch (args[0]) {
            case "--rpc" -> RpcMode.SOURCE;
            case "--bind-rpc" -> RpcMode.BIND;
            default -> null;
        };
    }
}
