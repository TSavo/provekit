// SPDX-License-Identifier: Apache-2.0
//
// Cross-kit byte-equivalence tests for the proof envelope. Pinned bytes
// from the rust kit, mirroring the python peer's
// implementations/python/.../tests/test_proof_envelope.py.
//
// The protocol IS the bytes; if any pin here drifts, the Java impl is
// wrong, not the test.

package com.provekit.proofenvelope;

import com.provekit.canonicalizer.Hash;

import org.junit.jupiter.api.Test;

import java.util.LinkedHashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

class ProofEnvelopeTest {

    private static final byte[] FOUNDATION_V0_SEED = new byte[32];
    static {
        for (int i = 0; i < 32; i++) FOUNDATION_V0_SEED[i] = 0x42;
    }

    private static byte[] foundationPubkey() {
        return Sign.ed25519PubkeyBytes(FOUNDATION_V0_SEED);
    }

    private static ProofEnvelope.Input minimalInput() {
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes());
        return ProofEnvelope.Input.builder()
            .name("@test/cat")
            .version("1.0.0")
            .members(members)
            .signerCid("blake3-512:cc")
            .declaredAt("2026-04-30T00:00:00.000Z")
            .signerSeed(FOUNDATION_V0_SEED)
            .build();
    }

    private static ProofEnvelope.Input twoMemberInput() {
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes());
        members.put("blake3-512:bb", "{\"goodbye\":\"world\"}".getBytes());
        return ProofEnvelope.Input.builder()
            .name("@test/cat")
            .version("1.0.0")
            .members(members)
            .signerCid("blake3-512:cc")
            .declaredAt("2026-04-30T00:00:00.000Z")
            .signerSeed(FOUNDATION_V0_SEED)
            .build();
    }

    // -----------------------------------------------------------------------
    // Round-trip
    // -----------------------------------------------------------------------

    @Test
    void buildThenVerify() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertTrue(ProofEnvelope.verify(out.bytes, out.cid, foundationPubkey()));
    }

    @Test
    void cidIsBlake3_512OfBytes() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertEquals(Hash.blake3_512(out.bytes), out.cid);
    }

    @Test
    void cidHasCorrectPrefix() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertTrue(out.cid.startsWith("blake3-512:"));
    }

    @Test
    void cidLengthIsPrefixPlus128Hex() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertEquals("blake3-512:".length() + 128, out.cid.length());
    }

    @Test
    void signedMapHeadIsSevenKeys() {
        // 7-key map head: major 5 (0xA0) + count 7 = 0xA7.
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertEquals((byte) 0xA7, out.bytes[0]);
    }

    @Test
    void deterministicAcrossRuns() {
        ProofEnvelope.Output a = ProofEnvelope.build(minimalInput());
        ProofEnvelope.Output b = ProofEnvelope.build(minimalInput());
        assertArrayEquals(a.bytes, b.bytes);
        assertEquals(a.cid, b.cid);
    }

    @Test
    void emptyMembersProducesValidEnvelope() {
        ProofEnvelope.Input inp = ProofEnvelope.Input.builder()
            .name("x")
            .version("1")
            .signerCid("blake3-512:cc")
            .declaredAt("2026-04-30T00:00:00.000Z")
            .signerSeed(FOUNDATION_V0_SEED)
            .build();
        ProofEnvelope.Output out = ProofEnvelope.build(inp);
        assertEquals((byte) 0xA7, out.bytes[0]);
        assertTrue(out.cid.startsWith("blake3-512:"));
        assertTrue(ProofEnvelope.verify(out.bytes, out.cid, foundationPubkey()));
    }

    @Test
    void changingNameChangesCid() {
        ProofEnvelope.Output a = ProofEnvelope.build(minimalInput());
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes());
        ProofEnvelope.Output b = ProofEnvelope.build(
            ProofEnvelope.Input.builder()
                .name("@other/name")
                .version("1.0.0")
                .members(members)
                .signerCid("blake3-512:cc")
                .declaredAt("2026-04-30T00:00:00.000Z")
                .signerSeed(FOUNDATION_V0_SEED)
                .build());
        assertNotEquals(a.cid, b.cid);
    }

    @Test
    void changingMembersChangesCid() {
        ProofEnvelope.Output a = ProofEnvelope.build(minimalInput());
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes());
        members.put("blake3-512:extra", "extra".getBytes());
        ProofEnvelope.Output b = ProofEnvelope.build(
            ProofEnvelope.Input.builder()
                .name("@test/cat")
                .version("1.0.0")
                .members(members)
                .signerCid("blake3-512:cc")
                .declaredAt("2026-04-30T00:00:00.000Z")
                .signerSeed(FOUNDATION_V0_SEED)
                .build());
        assertNotEquals(a.cid, b.cid);
    }

    @Test
    void changingSeedChangesCid() {
        byte[] alt = new byte[32];
        for (int i = 0; i < 32; i++) alt[i] = (byte) 0x99;
        ProofEnvelope.Output a = ProofEnvelope.build(minimalInput());
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "{\"hello\":\"world\"}".getBytes());
        ProofEnvelope.Output b = ProofEnvelope.build(
            ProofEnvelope.Input.builder()
                .name("@test/cat")
                .version("1.0.0")
                .members(members)
                .signerCid("blake3-512:cc")
                .declaredAt("2026-04-30T00:00:00.000Z")
                .signerSeed(alt)
                .build());
        assertNotEquals(a.cid, b.cid);
    }

    @Test
    void verifyRejectsTamperedCid() {
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        String fakeCid = "blake3-512:" + "00".repeat(64);
        assertFalse(ProofEnvelope.verify(out.bytes, fakeCid, foundationPubkey()));
    }

    @Test
    void verifyRejectsWrongPubkey() {
        byte[] wrongSeed = new byte[32];
        for (int i = 0; i < 32; i++) wrongSeed[i] = (byte) 0x99;
        byte[] wrongPubkey = Sign.ed25519PubkeyBytes(wrongSeed);
        ProofEnvelope.Output out = ProofEnvelope.build(minimalInput());
        assertFalse(ProofEnvelope.verify(out.bytes, out.cid, wrongPubkey));
    }

    @Test
    void binaryCidFieldIncludedInSignedBody() {
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "data".getBytes());
        ProofEnvelope.Input withBinary = ProofEnvelope.Input.builder()
            .name("@test/cat")
            .version("1.0.0")
            .members(members)
            .signerCid("blake3-512:cc")
            .declaredAt("2026-04-30T00:00:00.000Z")
            .signerSeed(FOUNDATION_V0_SEED)
            .binaryCid("blake3-512:deadbeef")
            .build();
        ProofEnvelope.Output out = ProofEnvelope.build(withBinary);
        assertTrue(ProofEnvelope.verify(out.bytes, out.cid, foundationPubkey()));

        ProofEnvelope.Input withoutBinary = ProofEnvelope.Input.builder()
            .name("@test/cat")
            .version("1.0.0")
            .members(members)
            .signerCid("blake3-512:cc")
            .declaredAt("2026-04-30T00:00:00.000Z")
            .signerSeed(FOUNDATION_V0_SEED)
            .build();
        ProofEnvelope.Output noBin = ProofEnvelope.build(withoutBinary);
        assertNotEquals(out.cid, noBin.cid);
    }

    @Test
    void metadataFieldIncludedInSignedBody() {
        Map<String, byte[]> members = new LinkedHashMap<>();
        members.put("blake3-512:aa", "data".getBytes());
        Map<String, String> metadata = new LinkedHashMap<>();
        metadata.put("tool", "java-kit");
        metadata.put("version", "0.1.0");
        ProofEnvelope.Input inp = ProofEnvelope.Input.builder()
            .name("@test/cat")
            .version("1.0.0")
            .members(members)
            .signerCid("blake3-512:cc")
            .declaredAt("2026-04-30T00:00:00.000Z")
            .signerSeed(FOUNDATION_V0_SEED)
            .metadata(metadata)
            .build();
        ProofEnvelope.Output out = ProofEnvelope.build(inp);
        assertTrue(ProofEnvelope.verify(out.bytes, out.cid, foundationPubkey()));
    }

    // -----------------------------------------------------------------------
    // Cross-kit byte-equivalence (pinned from rust + python reference)
    //
    // Pinned from:
    //   cargo run --release -p provekit-proof-envelope --example proof_envelope_bytes
    // Input: name=@test/cat, version=1.0.0, seed=[0x42;32],
    //        members={blake3-512:aa: '{"hello":"world"}',
    //                 blake3-512:bb: '{"goodbye":"world"}'},
    //        signer=blake3-512:cc, declaredAt=2026-04-30T00:00:00.000Z
    // Mirrored byte-for-byte from
    //   implementations/python/provekit-lift-py-tests/tests/test_proof_envelope.py
    // -----------------------------------------------------------------------

    private static final String RUST_FIXTURE_CID =
        "blake3-512:5ed1e1f705622ad52ae4683e3d12df5586d364d66bb3186f5be512415edf290"
        + "844d74e73a2857cd858f37803e4b11fe5c7cba7884caa6b9ff847521ce32ea056";

    private static final String RUST_FIXTURE_BYTES_HEX_FULL =
        "a7646b696e6467636174616c6f67646e616d656940746573742f636174667369676e65"
        + "726d626c616b65332d3531323a6363676d656d62657273a26d626c616b65332d353132"
        + "3a6161517b2268656c6c6f223a22776f726c64227d6d626c616b65332d3531323a6262"
        + "537b22676f6f64627965223a22776f726c64227d6776657273696f6e65312e302e3069"
        + "7369676e617475726558406a21dd428a54e22c82ca6d6125a7293c4a723786cb1840e8"
        + "91cefa03e63246eb97ef13dab86b7b1469d67302fadc969cd88c92c29495d13c75fc02"
        + "01a7263b066a6465636c6172656441747818323032362d30342d33305430303a30303a"
        + "30302e3030305a";

    @Test
    void twoMemberBytesMatchRust() {
        ProofEnvelope.Output out = ProofEnvelope.build(twoMemberInput());
        byte[] rustBytes = hexToBytes(RUST_FIXTURE_BYTES_HEX_FULL);
        if (!java.util.Arrays.equals(out.bytes, rustBytes)) {
            // Find first divergence for diagnostics.
            int n = Math.min(out.bytes.length, rustBytes.length);
            for (int i = 0; i < n; i++) {
                if (out.bytes[i] != rustBytes[i]) {
                    fail("cross-kit byte divergence at byte " + i + ": java=0x"
                        + String.format("%02x", out.bytes[i] & 0xFF)
                        + " rust=0x" + String.format("%02x", rustBytes[i] & 0xFF)
                        + "\njava hex: " + bytesToHex(out.bytes)
                        + "\nrust hex: " + bytesToHex(rustBytes));
                }
            }
            fail("cross-kit length mismatch: java=" + out.bytes.length
                + ", rust=" + rustBytes.length);
        }
    }

    @Test
    void twoMemberCidMatchesRust() {
        ProofEnvelope.Output out = ProofEnvelope.build(twoMemberInput());
        assertEquals(RUST_FIXTURE_CID, out.cid,
            "CID mismatch:\n  java: " + out.cid + "\n  rust: " + RUST_FIXTURE_CID);
    }

    @Test
    void rustBytesVerifyWithFoundationKey() {
        byte[] rustBytes = hexToBytes(RUST_FIXTURE_BYTES_HEX_FULL);
        assertTrue(ProofEnvelope.verify(rustBytes, RUST_FIXTURE_CID, foundationPubkey()),
            "Rust-produced bytes must verify under the Java verifier.");
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    private static byte[] hexToBytes(String hex) {
        byte[] out = new byte[hex.length() / 2];
        for (int i = 0; i < out.length; i++) {
            out[i] = (byte) Integer.parseInt(hex.substring(i * 2, i * 2 + 2), 16);
        }
        return out;
    }

    private static String bytesToHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }
}
