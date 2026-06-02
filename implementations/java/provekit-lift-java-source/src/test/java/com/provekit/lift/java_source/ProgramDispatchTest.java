package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import org.junit.jupiter.api.Test;

class ProgramDispatchTest {
    @Test
    void rpcFlagRoutesToJavaSourceServer() {
        Program.RpcMode mode = rpcMode("--rpc");

        assertEquals(Program.RpcMode.SOURCE, mode);
    }

    @Test
    void bindRpcFlagRoutesToBindServer() {
        Program.RpcMode mode = rpcMode("--bind-rpc");

        assertEquals(Program.RpcMode.BIND, mode);
    }

    @Test
    void unknownFlagHasNoRpcMode() {
        assertNull(rpcMode("--nope"));
    }

    private static Program.RpcMode rpcMode(String arg) {
        return Program.rpcMode(new String[] {arg});
    }
}
