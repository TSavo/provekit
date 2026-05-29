package com.provekit.emit.testng;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Jcs;

/** Loader-contract tests for the TestNG emitter's PEP 1.7.0 describe result. */
class RpcServerDescribeTest {

    private final RpcServer server = new RpcServer();

    @Test
    void describeResponseIsAnEnvelopedPluginMemento() {
        Jcs.Json doc = Jcs.parse(server.describeResult());
        assertTrue(doc instanceof Jcs.Obj, "describe result must be a JSON object");
        Jcs.Obj obj = (Jcs.Obj) doc;

        assertNotNull(obj.get("envelope"), "missing 'envelope'");
        assertNotNull(obj.get("header"), "missing 'header'");
        assertNotNull(obj.get("metadata"), "missing 'metadata'");
        assertTrue(obj.get("envelope") instanceof Jcs.Obj);
        assertTrue(obj.get("header") instanceof Jcs.Obj);
        assertTrue(obj.get("metadata") instanceof Jcs.Obj);
    }

    @Test
    void headerDeclaresTestNgEmitterCapability() {
        Jcs.Obj header = (Jcs.Obj) ((Jcs.Obj) Jcs.parse(server.describeResult())).get("header");
        assertEquals("1", header.stringFieldOrNull("schemaVersion"));
        assertEquals("emit", header.stringFieldOrNull("kind"));
        assertEquals(RpcServer.VERSION, header.stringFieldOrNull("version"));
        assertEquals(RpcServer.PROVENANCE_CID, header.stringFieldOrNull("provenance_cid"));

        Jcs.Obj content = (Jcs.Obj) header.get("content");
        assertEquals("java", content.stringFieldOrNull("target_language"));
        assertEquals("testng", content.stringFieldOrNull("target_framework"));
        assertEquals("testng-assertions", content.stringFieldOrNull("emits"));
    }

    @Test
    void headerCidEqualsRecomputedCid() {
        Jcs.Obj header = (Jcs.Obj) ((Jcs.Obj) Jcs.parse(server.describeResult())).get("header");
        assertEquals(RpcServer.computePluginCid(), header.stringFieldOrNull("cid"));
    }

    @Test
    void recomputedCidMatchesPinnedConstant() {
        String computed = RpcServer.computePluginCid();
        assertEquals(RpcServer.PLUGIN_CID, computed,
            "CONTENT_JSON changed without re-minting PLUGIN_CID; new cid = " + computed);
        assertTrue(computed.startsWith("blake3-512:"));
    }
}
