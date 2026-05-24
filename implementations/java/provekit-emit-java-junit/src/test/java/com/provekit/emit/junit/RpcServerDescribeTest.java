package com.provekit.emit.junit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Jcs;

/**
 * Gates the loader-contract fix (PR-6 review): the {@code describe} response
 * MUST be an enveloped plugin memento ({@code {envelope, header, metadata}})
 * whose {@code header.cid} the rust loader can recompute and verify, or the
 * kit is REFUSED at load (loader.rs {@code parse_and_validate}). The #1436
 * retirement gauntlet invokes this emitter through the standard plugin loader,
 * so a flat/unverifiable describe response would block the gauntlet.
 *
 * <p>These tests reproduce the loader's §6.1 CID recomputation in java
 * (provekit-ir {@link Jcs} + Blake3, byte-identical to rust's
 * {@code provekit_canonicalizer}) so drift between {@code CONTENT_JSON} and the
 * hardcoded {@code PLUGIN_CID} surfaces here instead of only in the rust loader
 * post-merge.
 */
class RpcServerDescribeTest {

    private final RpcServer server = new RpcServer();

    @Test
    void describeResponseIsAnEnvelopedPluginMemento() {
        Jcs.Json doc = Jcs.parse(server.describeResult());
        assertTrue(doc instanceof Jcs.Obj, "describe result must be a JSON object");
        Jcs.Obj obj = (Jcs.Obj) doc;

        // The three keys the loader's parse_and_validate requires.
        assertNotNull(obj.get("envelope"), "missing 'envelope'");
        assertNotNull(obj.get("header"), "missing 'header'");
        assertNotNull(obj.get("metadata"), "missing 'metadata'");

        assertTrue(obj.get("envelope") instanceof Jcs.Obj);
        assertTrue(obj.get("header") instanceof Jcs.Obj);
        assertTrue(obj.get("metadata") instanceof Jcs.Obj);
    }

    @Test
    void envelopeHasDeclaredAtSignatureSigner() {
        Jcs.Obj env = (Jcs.Obj) ((Jcs.Obj) Jcs.parse(server.describeResult())).get("envelope");
        assertNotNull(env.stringFieldOrNull("declaredAt"));
        assertTrue(env.stringFieldOrNull("signature").startsWith("ed25519:"));
        assertTrue(env.stringFieldOrNull("signer").startsWith("ed25519:"));
    }

    @Test
    void headerHasAllRequiredFieldsWithCorrectValues() {
        Jcs.Obj header = (Jcs.Obj) ((Jcs.Obj) Jcs.parse(server.describeResult())).get("header");
        assertEquals("1", header.stringFieldOrNull("schemaVersion"));
        assertEquals(RpcServer.KIND, header.stringFieldOrNull("kind"));
        assertEquals(RpcServer.VERSION, header.stringFieldOrNull("version"));
        assertEquals(RpcServer.PROVENANCE_CID, header.stringFieldOrNull("provenance_cid"));
        assertNotNull(header.get("content"));

        // protocol_versions must structurally include the loader's
        // RUNTIME_PROTOCOL_VERSIONS entry "pep/1.7.0", or the loader fails
        // with ProtocolVersionMismatch (loader.rs §5).
        assertTrue(header.get("protocol_versions") instanceof Jcs.Arr);
        Jcs.Arr versions = (Jcs.Arr) header.get("protocol_versions");
        boolean has170 = versions.values().stream()
            .anyMatch(v -> v instanceof Jcs.Str s && "pep/1.7.0".equals(s.value()));
        assertTrue(has170, "protocol_versions must include pep/1.7.0");
    }

    @Test
    void headerCidEqualsRecomputedCid() {
        // The loader recomputes header.cid via §6.1 and refuses on mismatch.
        // Recompute it here the same way and assert the declared cid matches.
        Jcs.Obj header = (Jcs.Obj) ((Jcs.Obj) Jcs.parse(server.describeResult())).get("header");
        String declared = header.stringFieldOrNull("cid");
        assertEquals(RpcServer.computePluginCid(), declared,
            "declared header.cid must equal the §6.1-recomputed cid");
    }

    @Test
    void recomputedCidMatchesPinnedConstant() {
        // Drift guard: if CONTENT_JSON (or any header field in the cid-input)
        // changes without re-minting, this fails and reports the new cid.
        String computed = RpcServer.computePluginCid();
        assertEquals(RpcServer.PLUGIN_CID, computed,
            "CONTENT_JSON changed without re-minting PLUGIN_CID; "
            + "re-run mint-plugin-cid and update the constant. new cid = " + computed);
        assertTrue(computed.startsWith("blake3-512:"));
    }
}
