// SPDX-License-Identifier: Apache-2.0
//
// Input validation tests for v1.4 BridgeDeclaration record constructors.
//
// Covers spec §1.R2: placeholder strings (pending-*:, deferred:*) and
// non-CID values are rejected at construction time with a clear error
// message, not deferred to a downstream NPE or serialization failure.
//
// Source of truth:
//   protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R2

package com.provekit.ir;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class BridgeV14ValidationTests {

    private static final String VALID_CID = "blake3-512:"
            + "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            + "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    // --------------------------------------------------------------------
    // BridgeTarget.Contract
    // --------------------------------------------------------------------

    @Test
    void contract_rejects_pending_placeholder() {
        IllegalArgumentException e = assertThrows(
                IllegalArgumentException.class,
                () -> new BridgeTarget.Contract("pending-foo:bar"));
        assertTrue(e.getMessage().contains("pending-"),
                "error message must mention the placeholder prefix");
        assertTrue(e.getMessage().contains("pending-foo:bar"),
                "error message must include the offending value");
    }

    @Test
    void contract_rejects_deferred_placeholder() {
        IllegalArgumentException e = assertThrows(
                IllegalArgumentException.class,
                () -> new BridgeTarget.Contract("deferred:something"));
        assertTrue(e.getMessage().contains("deferred:"),
                "error message must mention the placeholder prefix");
    }

    @Test
    void contract_rejects_non_cid() {
        IllegalArgumentException e = assertThrows(
                IllegalArgumentException.class,
                () -> new BridgeTarget.Contract("not-a-cid"));
        assertTrue(e.getMessage().contains("canonical CID"),
                "error message must mention the canonical CID grammar");
    }

    @Test
    void contract_rejects_empty_string() {
        IllegalArgumentException e = assertThrows(
                IllegalArgumentException.class,
                () -> new BridgeTarget.Contract(""));
        assertTrue(e.getMessage().contains("empty"),
                "error message must mention empty");
    }

    @Test
    void contract_rejects_non_128_hex() {
        // 127 hex chars -> one short of the required 128.
        String shortCid = "blake3-512:"
                + "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                + "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assertEquals(127, shortCid.split(":", 2)[1].length(),
                "test setup must produce exactly 127 hex chars");
        assertThrows(IllegalArgumentException.class,
                () -> new BridgeTarget.Contract(shortCid));
    }

    @Test
    void contract_accepts_valid_cid() {
        BridgeTarget.Contract c = new BridgeTarget.Contract(VALID_CID);
        assertEquals("contract", c.kind());
        assertEquals(VALID_CID, c.cid());
    }

    // --------------------------------------------------------------------
    // BridgeTarget.ContractSet
    // --------------------------------------------------------------------

    @Test
    void contract_set_rejects_pending_placeholder() {
        IllegalArgumentException e = assertThrows(
                IllegalArgumentException.class,
                () -> new BridgeTarget.ContractSet("pending-csharp-counterpart:foo"));
        assertTrue(e.getMessage().contains("pending-"),
                "error message must mention the placeholder prefix");
    }

    @Test
    void contract_set_rejects_deferred_placeholder() {
        assertThrows(IllegalArgumentException.class,
                () -> new BridgeTarget.ContractSet("deferred:phase-3"));
    }

    @Test
    void contract_set_rejects_non_cid() {
        assertThrows(IllegalArgumentException.class,
                () -> new BridgeTarget.ContractSet("garbage"));
    }

    @Test
    void contract_set_accepts_valid_cid() {
        BridgeTarget.ContractSet s = new BridgeTarget.ContractSet(VALID_CID);
        assertEquals("contractSet", s.kind());
        assertEquals(VALID_CID, s.cid());
    }

    // --------------------------------------------------------------------
    // BridgeHeaderV14.sourceContractCid
    // --------------------------------------------------------------------

    @Test
    void header_v14_rejects_pending_placeholder_in_source_contract_cid() {
        BridgeTarget target = new BridgeTarget.Contract(VALID_CID);
        IllegalArgumentException e = assertThrows(
                IllegalArgumentException.class,
                () -> BridgeHeaderV14.of("b", "symbol", "layer",
                        "pending-rust-counterpart:foo", target));
        assertTrue(e.getMessage().contains("sourceContractCid"),
                "error message must name the offending field");
        assertTrue(e.getMessage().contains("pending-"),
                "error message must mention the placeholder prefix");
    }

    @Test
    void header_v14_rejects_deferred_placeholder_in_source_contract_cid() {
        BridgeTarget target = new BridgeTarget.Contract(VALID_CID);
        assertThrows(IllegalArgumentException.class,
                () -> BridgeHeaderV14.of("b", "symbol", "layer",
                        "deferred:later", target));
    }

    @Test
    void header_v14_rejects_non_cid_in_source_contract_cid() {
        BridgeTarget target = new BridgeTarget.Contract(VALID_CID);
        assertThrows(IllegalArgumentException.class,
                () -> BridgeHeaderV14.of("b", "symbol", "layer",
                        "not-even-close", target));
    }

    @Test
    void header_v14_accepts_valid_source_contract_cid() {
        BridgeTarget target = new BridgeTarget.Contract(VALID_CID);
        BridgeHeaderV14 h = BridgeHeaderV14.of("b", "symbol", "layer",
                VALID_CID, target);
        assertNotNull(h);
        assertEquals(VALID_CID, h.sourceContractCid());
    }

    // --------------------------------------------------------------------
    // BridgeMetadataV14 (CID-valued optional fields)
    // --------------------------------------------------------------------

    @Test
    void metadata_v14_rejects_pending_placeholder_in_target_witness_cid() {
        IllegalArgumentException e = assertThrows(
                IllegalArgumentException.class,
                () -> new BridgeMetadataV14(
                        "pending-prover-witness:w",
                        null, null, null, null, null));
        assertTrue(e.getMessage().contains("targetWitnessCid"),
                "error message must name the offending field");
    }

    @Test
    void metadata_v14_rejects_deferred_in_target_binary_cid() {
        assertThrows(IllegalArgumentException.class,
                () -> new BridgeMetadataV14(
                        null, "deferred:binary", null, null, null, null));
    }

    @Test
    void metadata_v14_rejects_pending_placeholder_in_target_contract_set_cid() {
        assertThrows(IllegalArgumentException.class,
                () -> new BridgeMetadataV14(
                        null, null, null, "pending-csharp-set:foo", null, null));
    }

    @Test
    void metadata_v14_allows_null_cid_fields() {
        BridgeMetadataV14 m = new BridgeMetadataV14(
                null, null, "swift-kit", null,
                "provekit", "2026-05-03T00:00:00Z");
        assertNotNull(m);
    }

    @Test
    void metadata_v14_accepts_valid_cid_in_all_slots() {
        BridgeMetadataV14 m = new BridgeMetadataV14(
                VALID_CID, VALID_CID, null, VALID_CID,
                "provekit", "2026-05-03T00:00:00Z");
        assertEquals(VALID_CID, m.targetWitnessCid());
        assertEquals(VALID_CID, m.targetBinaryCid());
        assertEquals(VALID_CID, m.targetContractSetCid());
    }

    // --------------------------------------------------------------------
    // BridgeDeclarationV14 (composes the above; smoke-test pass-through)
    // --------------------------------------------------------------------

    @Test
    void declaration_v14_construction_succeeds_with_valid_inputs() {
        BridgeEnvelope env = new BridgeEnvelope(
                "ed25519:abc", "2026-05-03T00:00:00Z", "ed25519:def");
        BridgeHeaderV14 header = BridgeHeaderV14.of(
                "b", "symbol", "layer", VALID_CID,
                new BridgeTarget.Contract(VALID_CID));
        BridgeMetadataV14 meta = BridgeMetadataV14.empty();
        BridgeDeclarationV14 decl = new BridgeDeclarationV14(env, header, meta);
        assertNotNull(decl);
    }
}
