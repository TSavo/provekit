// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance tests for Java Ed25519. The foundation v0
// public-key string is pinned in every kit's
// .provekit/self-contracts-attestations/<lang>.json under the `signer`
// field. Java's derivation must produce the same string.

package com.provekit.proofenvelope;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class SignTest {

    private static final byte[] FOUNDATION_V0_SEED = new byte[32];
    static {
        for (int i = 0; i < 32; i++) FOUNDATION_V0_SEED[i] = 0x42;
    }

    /**
     * Foundation v0 pubkey, pinned from
     * .provekit/self-contracts-attestations/&lt;any kit&gt;.json
     * `signer` field. If this value diverges, every kit's attestation
     * is wrong — the discriminator says THIS Java derivation is wrong.
     */
    private static final String FOUNDATION_V0_PUBKEY_STRING =
        "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=";

    @Test
    void foundationV0SeedDerivesPinnedPubkey() {
        String pubkey = Sign.ed25519PubkeyString(FOUNDATION_V0_SEED);
        assertEquals(FOUNDATION_V0_PUBKEY_STRING, pubkey,
            "Java Ed25519 seed-to-pubkey derivation diverged from the "
            + "foundation v0 pin. The protocol is the bytes; this is a real bug.");
    }

    @Test
    void deterministicSignatureForFixedSeed() {
        byte[] seed = FOUNDATION_V0_SEED;
        byte[] a = Sign.ed25519SignWithSeed(seed, "hello".getBytes());
        byte[] b = Sign.ed25519SignWithSeed(seed, "hello".getBytes());
        assertArrayEquals(a, b);
    }

    @Test
    void signatureIs64Bytes() {
        byte[] sig = Sign.ed25519SignWithSeed(FOUNDATION_V0_SEED, "hello".getBytes());
        assertEquals(64, sig.length);
    }

    @Test
    void signStringHasPrefix() {
        String s = Sign.ed25519SignString(FOUNDATION_V0_SEED, "hello".getBytes());
        assertTrue(s.startsWith("ed25519:"));
    }

    @Test
    void pubkeyStringHasPrefixAnd44Base64Chars() {
        String pk = Sign.ed25519PubkeyString(FOUNDATION_V0_SEED);
        assertTrue(pk.startsWith("ed25519:"));
        // 32 raw bytes -> 44 base64 chars (with padding).
        assertEquals(44, pk.substring("ed25519:".length()).length());
    }

    @Test
    void verifyRoundTrip() {
        String pk = Sign.ed25519PubkeyString(FOUNDATION_V0_SEED);
        String sig = Sign.ed25519SignString(FOUNDATION_V0_SEED, "hello world".getBytes());
        assertTrue(Sign.ed25519VerifyString(pk, sig, "hello world".getBytes()));
        assertFalse(Sign.ed25519VerifyString(pk, sig, "goodbye world".getBytes()));
    }

    @Test
    void verifyRejectsMalformedInputs() {
        assertFalse(Sign.ed25519VerifyString("not-prefixed", "ed25519:AAAA==", new byte[0]));
        assertFalse(Sign.ed25519VerifyString("ed25519:AAAA==", "not-prefixed", new byte[0]));
        assertFalse(Sign.ed25519VerifyString("ed25519:!!!!", "ed25519:!!!!", new byte[0]));
    }

    @Test
    void differentSeedsProduceDifferentSignatures() {
        byte[] seedA = new byte[32];
        byte[] seedB = new byte[32];
        for (int i = 0; i < 32; i++) {
            seedA[i] = 0x42;
            seedB[i] = 0x43;
        }
        byte[] sigA = Sign.ed25519SignWithSeed(seedA, "same message".getBytes());
        byte[] sigB = Sign.ed25519SignWithSeed(seedB, "same message".getBytes());
        assertFalse(java.util.Arrays.equals(sigA, sigB));
    }

    @Test
    void differentMessagesProduceDifferentSignatures() {
        byte[] sigA = Sign.ed25519SignWithSeed(FOUNDATION_V0_SEED, "message a".getBytes());
        byte[] sigB = Sign.ed25519SignWithSeed(FOUNDATION_V0_SEED, "message b".getBytes());
        assertFalse(java.util.Arrays.equals(sigA, sigB));
    }

    @Test
    void rejectsBadSeedLength() {
        assertThrows(IllegalArgumentException.class,
            () -> Sign.ed25519SignWithSeed(new byte[31], new byte[0]));
        assertThrows(IllegalArgumentException.class,
            () -> Sign.ed25519PubkeyString(new byte[33]));
    }
}
