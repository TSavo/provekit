package com.provekit.emit.assertj;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Jcs;

class RpcServerDescribeTest {
    @Test
    void pluginCidMatchesInlineAssertJCapabilityDocument() {
        assertEquals(RpcServer.computePluginCid(), RpcServer.PLUGIN_CID);
    }

    @Test
    void describeAdvertisesAssertJOnly() {
        Jcs.Json parsed = Jcs.parse(new RpcServer().describeResult());
        assertTrue(parsed instanceof Jcs.Obj);
        Jcs.Obj obj = (Jcs.Obj) parsed;
        Jcs.Obj header = (Jcs.Obj) obj.get("header");
        Jcs.Obj content = (Jcs.Obj) header.get("content");

        assertEquals("emit", header.stringFieldOrNull("kind"));
        assertEquals("assertj", content.stringFieldOrNull("target_framework"));
        assertEquals("java", content.stringFieldOrNull("target_language"));
        assertTrue(Jcs.encode(content).contains("\"concept:eq\""));
        assertTrue(Jcs.encode(content).contains("\"concept:option-is-none\""));
    }
}
