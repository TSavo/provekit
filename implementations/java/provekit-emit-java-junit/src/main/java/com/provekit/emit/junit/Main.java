package com.provekit.emit.junit;

/**
 * Entry point for the provekit-emit-java-junit PEP 1.7.0 realize-plugin.
 *
 * <p>Usage: {@code provekit-emit-java-junit --rpc}
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
            System.err.println("Usage: provekit-emit-java-junit --rpc");
            System.exit(1);
        }
        new RpcServer().run();
    }
}
