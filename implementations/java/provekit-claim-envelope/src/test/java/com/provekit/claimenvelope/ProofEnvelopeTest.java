// SPDX-License-Identifier: Apache-2.0
//
// Round-trip + cross-kit byte-equivalence tests for the .proof envelope.
//
// The cross-kit pin is the exact two-member fixture pinned by the python
// kit (PR #221) from the rust reference output. If the Java output is
// not byte-identical to that fixture, the divergence is real, not a
// near-miss; surface it.

package com.provekit.claimenvelope;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.TreeMap;

import org.junit.jupiter.api.Test;

class ProofEnvelopeTest {

    /**
     * The python kit's pinned cross-kit fixture (cargo run --release -p
     * provekit-proof-envelope --example proof_envelope_bytes), exact
     * 252-byte signed-catalog output for a two-member input.
     *
     * Input: name=@test/cat, version=1.0.0, seed=[0x42;32],
     *        members={blake3-512:aa: '{"hello":"world"}',
     *                 blake3-512:bb: '{"goodbye":"world"}'},
     *        signer=blake3-512:cc, declaredAt=2026-04-30T00:00:00.000Z
     */
    private static final String RUST_FIXTURE_BYTES_HEX =
        "a7646b696e6467636174616c6f67646e616d656940746573742f636174667369676e65" +
        "726d626c616b65332d3531323a6363676d656d62657273a26d626c616b65332d353132" +
        "3a6161517b2268656c6c6f223a22776f726c64227d6d626c616b65332d3531323a6262" +
        "537b22676f6f64627965223a22776f726c64227d6776657273696f6e65312e302e3069" +
        "7369676e617475726558406a21dd428a54e22c82ca6d6125a7293c4a723786cb1840e8" +
        "91cefa03e63246eb97ef13dab86b7b1469d67302fadc969cd88c92c29495d13c75fc02" +
        "01a7263b066a6465636c6172656441747818323032362d30342d33305430303a30303a" +
        "30302e3030305a";

    private static final String RUST_FIXTURE_CID =
        "blake3-512:5ed1e1f705622ad52ae4683e3d12df5586d364d66bb3186f5be512415edf290" +
        "844d74e73a2857cd858f37803e4b11fe5c7cba7884caa6b9ff847521ce32ea056";

    private static byte[] hex2bytes(String hex) {
        int n = hex.length();
        byte[] out = new byte[n / 2];
        for (int i = 0; i < n; i += 2) {
            out[i / 2] = (byte) Integer.parseInt(hex.substring(i, i + 2), 16);
        }
        return out;
    }

    private static ProofEnvelope.Input minimalInput() {
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes(StandardCharsets.UTF_8));
        return new ProofEnvelope.Input(
            "@test/cat",
            "1.0.0",
            members,
            "blake3-512:cc",
            Ed25519.FOUNDATION_V0_SEED,
            "2026-04-30T00:00:00.000Z"
        );
    }

    private static ProofEnvelope.Input twoMemberInput() {
        // Use TreeMap so iteration order is deterministic; the encoder sorts
        // by CBOR-encoded-key bytes anyway, but matching python's input
        // construction keeps the two impls obviously parallel.
        Map<String, byte[]> members = new TreeMap<>();
        members.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes(StandardCharsets.UTF_8));
        members.put("blake3-512:bb", "{\"goodbye\":\"world\"}".getBytes(StandardCharsets.UTF_8));
        return new ProofEnvelope.Input(
            "@test/cat",
            "1.0.0",
            members,
            "blake3-512:cc",
            Ed25519.FOUNDATION_V0_SEED,
            "2026-04-30T00:00:00.000Z"
        );
    }

    // -------------------------------------------------------------------
    // Round-trip
    // -------------------------------------------------------------------

    @Test
    void build_then_verify() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        byte[] pk = Ed25519.pubkeyBytes(Ed25519.FOUNDATION_V0_SEED);
        assertTrue(ProofEnvelope.verify(out.bytes, out.cid, pk));
    }

    @Test
    void cid_is_blake3_512_of_bytes() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertEquals(out.cid, Blake3.blake3_512(out.bytes));
    }

    @Test
    void cid_has_correct_prefix_and_length() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertTrue(out.cid.startsWith("blake3-512:"));
        assertEquals("blake3-512:".length() + 128, out.cid.length());
    }

    @Test
    void signed_map_head_is_seven_keys() {
        // 7-key map head: major 5 (0xA0) + count 7 = 0xA7.
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertEquals((byte) 0xA7, out.bytes[0]);
    }

    @Test
    void deterministic_across_runs() {
        ProofEnvelope.Output a = ProofEnvelope.build(minimalInput());
        ProofEnvelope.Output b = ProofEnvelope.build(minimalInput());
        assertArrayEquals(a.bytes, b.bytes);
        assertEquals(a.cid, b.cid);
    }

    @Test
    void empty_members_produces_valid_envelope() {
        ProofEnvelope.Input inp = new ProofEnvelope.Input(
            "x",
            "1",
            new LinkedHashMap<>(),
            "blake3-512:cc",
            Ed25519.FOUNDATION_V0_SEED,
            "2026-04-30T00:00:00.000Z"
        );
        ProofEnvelope.Output out = ProofEnvelope.build(inp);
        assertEquals((byte) 0xA7, out.bytes[0]);
        assertTrue(out.cid.startsWith("blake3-512:"));
        byte[] pk = Ed25519.pubkeyBytes(Ed25519.FOUNDATION_V0_SEED);
        assertTrue(ProofEnvelope.verify(out.bytes, out.cid, pk));
    }

    @Test
    void two_members_round_trip() {
        ProofEnvelope.Output out = ProofEnvelope.build(twoMemberInput());
        byte[] pk = Ed25519.pubkeyBytes(Ed25519.FOUNDATION_V0_SEED);
        assertTrue(ProofEnvelope.verify(out.bytes, out.cid, pk));
    }

    @Test
    void changing_name_changes_cid() {
        ProofEnvelope.Output a = ProofEnvelope.build(minimalInput());
        Map<String, byte[]> m = new LinkedHashMap<>();
        m.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes(StandardCharsets.UTF_8));
        ProofEnvelope.Input b = new ProofEnvelope.Input(
            "@other/name", "1.0.0", m, "blake3-512:cc",
            Ed25519.FOUNDATION_V0_SEED, "2026-04-30T00:00:00.000Z"
        );
        assertNotEquals(a.cid, ProofEnvelope.build(b).cid);
    }

    @Test
    void verify_rejects_tampered_cid() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        StringBuilder fake = new StringBuilder("blake3-512:");
        for (int i = 0; i < 128; i++) fake.append('0');
        byte[] pk = Ed25519.pubkeyBytes(Ed25519.FOUNDATION_V0_SEED);
        assertFalse(ProofEnvelope.verify(out.bytes, fake.toString(), pk));
    }

    @Test
    void verify_rejects_wrong_pubkey() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        byte[] wrongSeed = new byte[32];
        for (int i = 0; i < 32; i++) wrongSeed[i] = (byte) 0x99;
        byte[] wrongPk = Ed25519.pubkeyBytes(wrongSeed);
        assertFalse(ProofEnvelope.verify(out.bytes, out.cid, wrongPk));
    }

    // -------------------------------------------------------------------
    // Cross-kit byte-equivalence — THE LOAD-BEARING TEST
    // -------------------------------------------------------------------

    @Test
    void two_member_bytes_match_rust_fixture() {
        ProofEnvelope.Output out = ProofEnvelope.build(twoMemberInput());
        byte[] expected = hex2bytes(RUST_FIXTURE_BYTES_HEX);
        if (!java.util.Arrays.equals(out.bytes, expected)) {
            int n = Math.min(out.bytes.length, expected.length);
            for (int i = 0; i < n; i++) {
                if (out.bytes[i] != expected[i]) {
                    fail(String.format(
                        "cross-kit byte divergence at byte %d: java=0x%02x rust=0x%02x%n" +
                        "java hex: %s%nrust hex: %s",
                        i, out.bytes[i] & 0xFF, expected[i] & 0xFF,
                        bytesToHex(out.bytes), RUST_FIXTURE_BYTES_HEX));
                }
            }
            fail(String.format("cross-kit length mismatch: java=%d rust=%d",
                out.bytes.length, expected.length));
        }
    }

    @Test
    void two_member_cid_matches_rust_fixture() {
        ProofEnvelope.Output out = ProofEnvelope.build(twoMemberInput());
        assertEquals(RUST_FIXTURE_CID, out.cid);
    }

    @Test
    void rust_bytes_verify_with_foundation_pubkey() {
        // The rust-produced bytes (from the fixture) must verify under our
        // Java verifier using the foundation v0 pubkey.
        byte[] rustBytes = hex2bytes(RUST_FIXTURE_BYTES_HEX);
        byte[] pk = Ed25519.pubkeyBytes(Ed25519.FOUNDATION_V0_SEED);
        assertTrue(ProofEnvelope.verify(rustBytes, RUST_FIXTURE_CID, pk));
    }

    private static String bytesToHex(byte[] b) {
        StringBuilder sb = new StringBuilder(b.length * 2);
        for (byte x : b) sb.append(String.format("%02x", x & 0xFF));
        return sb.toString();
    }
}
