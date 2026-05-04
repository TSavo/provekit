// SPDX-License-Identifier: Apache-2.0
//
// Round-trip type tests for the v1.4 BridgeDeclaration types.
//
// Source of truth:
//   protocol/specs/2026-05-03-bridge-target-dimensionality.md
//   protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md
//   protocol/provekit-ir.cddl  (BridgeDeclarationV14, BridgeTarget, ...)
//
// Mirrors implementations/rust/provekit-ir-types/tests/bridge_v14_serde.rs.
// Byte-equality round-trip lives in the claim-envelope module's
// BridgeV14RoundtripTest because this module has no JCS encoder.

package com.provekit.ir;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import org.junit.jupiter.api.Test;

class BridgeV14SerdeTest {

    @Test
    void bridge_v14_record_carries_seven_header_fields() {
        BridgeTarget target = new BridgeTarget.Contract(
                "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111");
        BridgeHeaderV14 header = BridgeHeaderV14.of(
                "rust-canonical-bridge-fixture",
                "parseInt",
                "rust-kit",
                "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                target);

        // Spec §1.R3: header carries the seven contract-axis fields only.
        assertEquals("1", header.schemaVersion());
        assertEquals("bridge", header.kind());
        assertEquals("rust-canonical-bridge-fixture", header.name());
        assertEquals("parseInt", header.sourceSymbol());
        assertEquals("rust-kit", header.sourceLayer());
        assertTrue(header.sourceContractCid().startsWith("blake3-512:000"));
        assertNotNull(header.target());
    }

    @Test
    void bridge_v14_target_discriminates_on_kind() {
        BridgeTarget tContract = new BridgeTarget.Contract(
                "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc");
        BridgeTarget tSet = new BridgeTarget.ContractSet(
                "blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

        // The sealed interface forces exactly one variant; pattern-match
        // semantics line up with serde's tagged-union deserialize.
        if (tContract instanceof BridgeTarget.Contract c) {
            assertEquals("contract", c.kind());
            assertTrue(c.cid().startsWith("blake3-512:cccccccc"));
        } else {
            fail("expected Contract variant");
        }

        if (tSet instanceof BridgeTarget.ContractSet s) {
            assertEquals("contractSet", s.kind());
            assertTrue(s.cid().startsWith("blake3-512:eeeeeeee"));
        } else {
            fail("expected ContractSet variant");
        }
    }

    @Test
    void bridge_v14_metadata_empty_factory_omits_all() {
        // Spec §1.R2: absent metadata fields are OMITTED, not null and
        // not stringified placeholders. The empty() factory matches the
        // rust BridgeMetadataV14::default() shape.
        BridgeMetadataV14 m = BridgeMetadataV14.empty();
        assertNull(m.targetWitnessCid());
        assertNull(m.targetBinaryCid());
        assertNull(m.targetLayer());
        assertNull(m.targetContractSetCid());
        assertNull(m.producedBy());
        assertNull(m.producedAt());
    }

    @Test
    void bridge_v14_full_record_compose() {
        BridgeEnvelope env = new BridgeEnvelope(
                "ed25519:pubkey-fixture-bytes",
                "2026-05-03T00:00:00.000Z",
                "ed25519:signature-fixture-bytes");
        BridgeHeaderV14 header = BridgeHeaderV14.of(
                "rust-canonical-bridge-fixture",
                "parseInt",
                "rust-kit",
                "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                new BridgeTarget.Contract("blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"));
        BridgeMetadataV14 meta = new BridgeMetadataV14(
                null, null, "rust-kit", null,
                "provekit-canonical-reference@v1.4",
                "2026-05-03T00:00:00.000Z");
        BridgeDeclarationV14 decl = new BridgeDeclarationV14(env, header, meta);

        assertEquals("ed25519:pubkey-fixture-bytes", decl.envelope().signer());
        assertEquals("rust-canonical-bridge-fixture", decl.header().name());
        assertEquals("rust-kit", decl.metadata().targetLayer());
        assertNull(decl.metadata().targetWitnessCid());
        assertNull(decl.metadata().targetBinaryCid());
        assertNull(decl.metadata().targetContractSetCid());

        // Tagged-union target round-trip on the typed shape.
        if (decl.header().target() instanceof BridgeTarget.Contract c) {
            assertTrue(c.cid().startsWith("blake3-512:11111111"));
        } else {
            fail("expected Contract variant");
        }
    }
}
