package com.provekit.emit.testng;

/** CLI entry point for the TestNG emitter artifact. */
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
            System.err.println("provekit-emit-java-testng expects --rpc");
            return;
        }
        new RpcServer().run();
    }
}
