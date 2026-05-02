package com.provekit.lift;

public class Main {
    public static void main(String[] args) {
        boolean rpc = false;
        for (String arg : args) {
            if ("--rpc".equals(arg)) { rpc = true; break; }
        }
        if (!rpc) {
            System.err.println("Usage: provekit-lift-java --rpc");
            System.exit(1);
        }
        new RpcServer().run();
    }
}
