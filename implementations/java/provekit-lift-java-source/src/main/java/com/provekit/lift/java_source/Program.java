package com.provekit.lift.java_source;

public final class Program {
    private Program() {}

    public static void main(String[] args) throws Exception {
        if (args.length == 1 && "--rpc".equals(args[0])) {
            RpcServer.run();
            return;
        }
        System.err.println("usage: java -jar provekit-lift-java-source.jar --rpc");
        System.exit(1);
    }
}
