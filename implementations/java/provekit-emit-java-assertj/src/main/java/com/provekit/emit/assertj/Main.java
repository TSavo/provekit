package com.provekit.emit.assertj;

/**
 * Entry point for the provekit-emit-java-assertj PEP 1.7.0 emit-plugin.
 *
 * <p>Usage: {@code provekit-emit-java-assertj --rpc}
 *
 * <p>Runs a newline-delimited JSON-RPC server on stdin/stdout (see
 * {@link RpcServer}).
 */
public final class Main {
    private Main() {}

    public static void main(String[] args) {
        boolean rpc = false;
        for (String arg : args) {
            if ("--rpc".equals(arg)) {
                rpc = true;
                break;
            }
        }
        if (!rpc) {
            System.err.println("Usage: provekit-emit-java-assertj --rpc");
            System.exit(1);
        }
        new RpcServer().run();
    }
}
