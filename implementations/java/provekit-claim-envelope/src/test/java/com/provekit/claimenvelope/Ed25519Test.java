// SPDX-License-Identifier: Apache-2.0

package com.provekit.claimenvelope;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class Ed25519Test {

    private static byte[] seed42() {
        byte[] s = new byte[32];
        for (int i = 0; i < 32; i++) s[i] = (byte) 0x42;
        return s;
    }

    @Test
    void deterministic_signature_for_fixed_seed() {
        byte[] seed = seed42();
        byte[] a = Ed25519.signWithSeed(seed, "hello".getBytes());
        byte[] b = Ed25519.signWithSeed(seed, "hello".getBytes());
        assertArrayEquals(a, b);
    }

    @Test
    void signature_is_64_bytes() {
        byte[] sig = Ed25519.signWithSeed(seed42(), "hello".getBytes());
        assertEquals(64, sig.length);
    }

    @Test
    void sign_string_has_prefix() {
        String s = Ed25519.signString(seed42(), "hello".getBytes());
        assertTrue(s.startsWith("ed25519:"));
    }

    @Test
    void pubkey_string_has_prefix() {
        String pk = Ed25519.pubkeyString(seed42());
        assertTrue(pk.startsWith("ed25519:"));
    }

    @Test
    void pubkey_base64_is_44_chars() {
        // 32 bytes -> 44 base64 chars
        String pk = Ed25519.pubkeyString(seed42());
        String b64 = pk.substring("ed25519:".length());
        assertEquals(44, b64.length());
    }

    @Test
    void verify_round_trip() {
        byte[] seed = seed42();
        String pk = Ed25519.pubkeyString(seed);
        String sig = Ed25519.signString(seed, "hello world".getBytes());
        assertTrue(Ed25519.verifyString(pk, sig, "hello world".getBytes()));
        assertFalse(Ed25519.verifyString(pk, sig, "goodbye world".getBytes()));
    }

    @Test
    void verify_rejects_malformed() {
        assertFalse(Ed25519.verifyString("not-prefixed", "ed25519:AAAA", "x".getBytes()));
        assertFalse(Ed25519.verifyString("ed25519:AAAA", "not-prefixed", "x".getBytes()));
        assertFalse(Ed25519.verifyString("ed25519:!!!!", "ed25519:!!!!", "x".getBytes()));
    }

    @Test
    void foundation_v0_seed_is_42_repeated() {
        byte[] seed = Ed25519.FOUNDATION_V0_SEED;
        assertEquals(32, seed.length);
        for (byte b : seed) assertEquals((byte) 0x42, b);
    }

    @Test
    void different_seeds_produce_different_signatures() {
        byte[] seedA = new byte[32];
        for (int i = 0; i < 32; i++) seedA[i] = (byte) 0x42;
        byte[] seedB = new byte[32];
        for (int i = 0; i < 32; i++) seedB[i] = (byte) 0x43;
        byte[] sigA = Ed25519.signWithSeed(seedA, "same".getBytes());
        byte[] sigB = Ed25519.signWithSeed(seedB, "same".getBytes());
        assertFalse(java.util.Arrays.equals(sigA, sigB));
    }

    @Test
    void verify_bytes_round_trip() {
        byte[] seed = seed42();
        byte[] pk = Ed25519.pubkeyBytes(seed);
        byte[] sig = Ed25519.signWithSeed(seed, "hello".getBytes());
        assertTrue(Ed25519.verifyBytes(pk, sig, "hello".getBytes()));
        assertFalse(Ed25519.verifyBytes(pk, sig, "world".getBytes()));
    }
}
