// SPDX-License-Identifier: Apache-2.0

package com.provekit.claimenvelope;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.provekit.claimenvelope.ClaimEnvelope.Authoring;
import com.provekit.claimenvelope.ClaimEnvelope.MintContractArgs;
import com.provekit.claimenvelope.ClaimEnvelope.MintedEnvelope;
import com.provekit.ir.Jcs.Value;

/**
 * Mirrors implementations/rust/provekit-claim-envelope/src/lib.rs unit tests.
 */
class ClaimEnvelopeTest {

    private static byte[] dummySeed() {
        byte[] s = new byte[32];
        for (int i = 0; i < 32; i++) s[i] = (byte) 0x42;
        return s;
    }

    @Test
    void empty_contract_rejected() {
        MintContractArgs args = new MintContractArgs();
        args.contractName = "x";
        args.outBinding = "out";
        args.producedBy = "test";
        args.producedAt = "2026-04-30T00:00:00.000Z";
        args.authoring = new Authoring.KitAuthor("test", null);
        args.signerSeed = dummySeed();
        assertThrows(IllegalArgumentException.class, () -> ClaimEnvelope.mintContract(args));
    }

    @Test
    void cid_is_blake3_512_prefixed() {
        Value pre = Value.object(
            "kind", Value.string("atomic"),
            "name", Value.string(">"),
            "args", Value.array(
                Value.object("kind", Value.string("var"), "name", Value.string("n")),
                Value.object(
                    "kind", Value.string("const"),
                    "value", Value.integer(0),
                    "sort", Value.object(
                        "kind", Value.string("primitive"),
                        "name", Value.string("Int")))
            ));
        MintContractArgs args = new MintContractArgs();
        args.contractName = "parseInt";
        args.pre = pre;
        args.outBinding = "out";
        args.producedBy = "java-kit@1.0";
        args.producedAt = "2026-04-30T00:00:00.000Z";
        args.authoring = new Authoring.KitAuthor("java-kit@1.0", null);
        args.signerSeed = dummySeed();

        MintedEnvelope m = ClaimEnvelope.mintContract(args);
        assertTrue(m.cid.startsWith("blake3-512:"));
        assertEquals("blake3-512:".length() + 128, m.cid.length());
    }

    @Test
    void contract_cid_signer_independent() {
        // Two different seeds, same contract content -> same contractCid,
        // different attestation cid.
        MintContractArgs argsA = new MintContractArgs();
        argsA.contractName = "p";
        argsA.pre = Value.object("k", Value.string("v"));
        argsA.outBinding = "out";
        argsA.producedBy = "a";
        argsA.producedAt = "2026-04-30T00:00:00.000Z";
        argsA.authoring = new Authoring.KitAuthor("a", null);
        argsA.signerSeed = dummySeed();

        MintContractArgs argsB = new MintContractArgs();
        argsB.contractName = "p";
        argsB.pre = Value.object("k", Value.string("v"));
        argsB.outBinding = "out";
        argsB.producedBy = "b";
        argsB.producedAt = "2026-04-30T00:00:00.000Z";
        argsB.authoring = new Authoring.KitAuthor("b", null);
        byte[] seedB = new byte[32];
        for (int i = 0; i < 32; i++) seedB[i] = (byte) 0x43;
        argsB.signerSeed = seedB;

        String ccidA = ClaimEnvelope.contractCid(argsA);
        String ccidB = ClaimEnvelope.contractCid(argsB);
        assertEquals(ccidA, ccidB);

        MintedEnvelope a = ClaimEnvelope.mintContract(argsA);
        MintedEnvelope b = ClaimEnvelope.mintContract(argsB);
        assertEquals(ccidA, a.contractCid);
        assertEquals(ccidB, b.contractCid);
        // Attestation cids differ (different signers).
        assertNotEquals(a.cid, b.cid);
    }

    @Test
    void contract_set_cid_order_independent() {
        String c1 = "blake3-512:" + "11".repeat(64);
        String c2 = "blake3-512:" + "22".repeat(64);
        String c3 = "blake3-512:" + "33".repeat(64);
        String setAB = ClaimEnvelope.computeContractSetCid(List.of(c1, c2, c3));
        String setBA = ClaimEnvelope.computeContractSetCid(List.of(c3, c1, c2));
        assertEquals(setAB, setBA);
    }

    @Test
    void mint_contract_deterministic() {
        MintContractArgs argsA = new MintContractArgs();
        argsA.contractName = "p";
        argsA.pre = Value.object("k", Value.string("v"));
        argsA.outBinding = "out";
        argsA.producedBy = "java-kit";
        argsA.producedAt = "2026-04-30T00:00:00.000Z";
        argsA.authoring = new Authoring.KitAuthor("java-kit", null);
        argsA.signerSeed = dummySeed();

        MintContractArgs argsB = new MintContractArgs();
        argsB.contractName = "p";
        argsB.pre = Value.object("k", Value.string("v"));
        argsB.outBinding = "out";
        argsB.producedBy = "java-kit";
        argsB.producedAt = "2026-04-30T00:00:00.000Z";
        argsB.authoring = new Authoring.KitAuthor("java-kit", null);
        argsB.signerSeed = dummySeed();

        MintedEnvelope a = ClaimEnvelope.mintContract(argsA);
        MintedEnvelope b = ClaimEnvelope.mintContract(argsB);
        assertArrayEquals(a.canonicalBytes, b.canonicalBytes);
        assertEquals(a.cid, b.cid);
    }
}
