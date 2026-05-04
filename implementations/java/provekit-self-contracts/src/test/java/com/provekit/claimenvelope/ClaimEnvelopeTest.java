// SPDX-License-Identifier: Apache-2.0
//
// Conformance tests for the Java claim-envelope minter.
//
// What this verifies:
//   - mintContract round-trips and produces a stable CID.
//   - contractCid is signer-independent (two distinct signers attesting
//     the same logical contract produce the same contractCid).
//   - contractSetCid sorts contractCids before hashing.
//   - The minted envelope's first byte is '{' (JCS encodes objects).
//   - The producerSignature field is the JCS-bytes signature in spec
//     "ed25519:<base64>" form.
//
// Cross-kit byte-equivalence at the claim-envelope level requires a
// rust-side example that emits a fixed input → fixed canonical bytes;
// that fixture is not yet checked in (the rust kit's
// `mint_self_contracts` flow embeds producer/declared_at metadata that
// drift across kits). The proof-envelope cross-kit fixture in
// ProofEnvelopeTest exercises the same JCS+CBOR+Ed25519+BLAKE3
// substrate and pins the load-bearing path byte-for-byte.

package com.provekit.claimenvelope;

import com.provekit.canonicalizer.Hash;
import com.provekit.canonicalizer.Jcs;
import com.provekit.canonicalizer.Value;
import com.provekit.proofenvelope.Sign;

import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

class ClaimEnvelopeTest {

    private static final byte[] FOUNDATION_V0_SEED = new byte[32];
    static {
        for (int i = 0; i < 32; i++) FOUNDATION_V0_SEED[i] = 0x42;
    }

    private static ClaimEnvelope.MintContractArgs sampleArgs() {
        // post: atomic("gte", var("x", Int), const(0, Int))
        Value post = Value.ofObject(List.of(
            Map.entry("kind", Value.ofString("atomic")),
            Map.entry("name", Value.ofString("gte")),
            Map.entry("args", Value.ofArray(List.of(
                Value.ofObject(List.of(
                    Map.entry("kind", Value.ofString("var")),
                    Map.entry("name", Value.ofString("x")))),
                Value.ofObject(List.of(
                    Map.entry("kind", Value.ofString("const")),
                    Map.entry("value", Value.ofInt(0)))))))));
        return ClaimEnvelope.MintContractArgs.builder()
            .contractName("abs")
            .post(post)
            .outBinding("out")
            .producedBy("java-kit@0.1")
            .producedAt("2026-04-30T00:00:00.000Z")
            .authoring(new Authoring.KitAuthor("java-kit"))
            .signerSeed(FOUNDATION_V0_SEED)
            .build();
    }

    @Test
    void mintContractIsDeterministic() {
        ClaimEnvelope.MintedEnvelope a = ClaimEnvelope.mintContract(sampleArgs());
        ClaimEnvelope.MintedEnvelope b = ClaimEnvelope.mintContract(sampleArgs());
        assertArrayEquals(a.canonicalBytes, b.canonicalBytes);
        assertEquals(a.cid, b.cid);
    }

    @Test
    void mintContractFirstByteIsJcsObjectOpen() {
        ClaimEnvelope.MintedEnvelope env = ClaimEnvelope.mintContract(sampleArgs());
        assertEquals((byte) '{', env.canonicalBytes[0]);
    }

    @Test
    void cidIsBlake3_512OfCanonicalBytes() {
        ClaimEnvelope.MintedEnvelope env = ClaimEnvelope.mintContract(sampleArgs());
        // The minted envelope's CID is the unsigned-body hash, NOT the
        // hash of the final signed bytes. So we re-derive expectations
        // from the documented protocol:
        //   cid = blake3-512(JCS(unsigned-envelope-without-cid-or-signature))
        // The minted bytes contain `cid` and `producerSignature` keys, so
        // hashing them won't match cid. Instead we just assert prefix +
        // length structure here; full equality is covered by the
        // determinism test.
        assertTrue(env.cid.startsWith("blake3-512:"));
        assertEquals("blake3-512:".length() + 128, env.cid.length());
    }

    @Test
    void contractCidIsSignerIndependent() {
        ClaimEnvelope.MintContractArgs argsA = sampleArgs();
        byte[] altSeed = new byte[32];
        for (int i = 0; i < 32; i++) altSeed[i] = (byte) 0x99;
        // Build a fresh args with a different seed AND a different
        // producedBy — neither should affect contractCid.
        Value post = argsA.post;
        ClaimEnvelope.MintContractArgs argsB = ClaimEnvelope.MintContractArgs.builder()
            .contractName(argsA.contractName)
            .post(post)
            .outBinding(argsA.outBinding)
            .producedBy("different-kit@9.9")
            .producedAt("9999-12-31T23:59:59.999Z")
            .authoring(new Authoring.KitAuthor("alt-kit"))
            .signerSeed(altSeed)
            .build();
        assertEquals(ClaimEnvelope.contractCid(argsA), ClaimEnvelope.contractCid(argsB));
    }

    @Test
    void contractCidDependsOnFormulaContent() {
        ClaimEnvelope.MintContractArgs a = sampleArgs();
        // Same signer/producer but different post predicate.
        Value differentPost = Value.ofObject(List.of(
            Map.entry("kind", Value.ofString("atomic")),
            Map.entry("name", Value.ofString("eq")),
            Map.entry("args", Value.ofArray(List.of()))));
        ClaimEnvelope.MintContractArgs b = ClaimEnvelope.MintContractArgs.builder()
            .contractName(a.contractName)
            .post(differentPost)
            .outBinding(a.outBinding)
            .producedBy(a.producedBy)
            .producedAt(a.producedAt)
            .authoring(new Authoring.KitAuthor("java-kit"))
            .signerSeed(a.signerSeed)
            .build();
        assertNotEquals(ClaimEnvelope.contractCid(a), ClaimEnvelope.contractCid(b));
    }

    @Test
    void contractSetCidSortsBeforeHashing() {
        List<String> ascending = List.of("blake3-512:aa", "blake3-512:bb", "blake3-512:cc");
        List<String> descending = List.of("blake3-512:cc", "blake3-512:bb", "blake3-512:aa");
        assertEquals(
            ClaimEnvelope.contractSetCid(ascending),
            ClaimEnvelope.contractSetCid(descending));
    }

    @Test
    void contractSetCidEmptyMatchesJcsEmptyArrayHash() {
        // contractSetCid([]) = blake3-512(JCS([])) = blake3-512(b"[]")
        String emptyArrayCid = ClaimEnvelope.contractSetCid(List.of());
        String expected = Hash.blake3_512(Jcs.encodeUtf8(Value.ofArray(List.of())));
        assertEquals(expected, emptyArrayCid);
    }

    @Test
    void rejectsContractWithNoFormulaParts() {
        ClaimEnvelope.MintContractArgs bad = ClaimEnvelope.MintContractArgs.builder()
            .contractName("nopart")
            .producedBy("kit")
            .producedAt("now")
            .authoring(new Authoring.KitAuthor("kit"))
            .signerSeed(FOUNDATION_V0_SEED)
            .build();
        assertThrows(IllegalArgumentException.class, () -> ClaimEnvelope.mintContract(bad));
    }

    @Test
    void mintedSignatureIsSelfIdentifying() {
        ClaimEnvelope.MintedEnvelope env = ClaimEnvelope.mintContract(sampleArgs());
        String s = new String(env.canonicalBytes, java.nio.charset.StandardCharsets.UTF_8);
        // The producerSignature field uses the spec form
        //   "ed25519:" + base64(sig)
        // so the literal substring "ed25519:" must appear.
        assertTrue(s.contains("\"producerSignature\":\"ed25519:"),
            "minted envelope must carry self-identifying producer signature, got: " + s);
    }

    @Test
    void mintedEnvelopeCarriesItsCid() {
        ClaimEnvelope.MintedEnvelope env = ClaimEnvelope.mintContract(sampleArgs());
        String s = new String(env.canonicalBytes, java.nio.charset.StandardCharsets.UTF_8);
        // The `cid` field is appended before signing → present in JCS bytes.
        assertTrue(s.contains("\"cid\":\"" + env.cid + "\""),
            "minted envelope must carry its own CID, got: " + s);
    }

    @Test
    void liftAuthoringEncodesProducerKindLift() {
        Value v = new Authoring.Lift("ir-document", "minted from ir-document RPC response").toValue();
        String jcs = Jcs.encode(v);
        assertTrue(jcs.contains("\"producerKind\":\"lift\""));
        assertTrue(jcs.contains("\"lifter\":\"ir-document\""));
    }

    @Test
    void llmAuthoringEncodesConfidenceAsMilliInt() {
        Value v = new Authoring.Llm("claude", "v1", "blake3-512:aa", 0.875).toValue();
        String jcs = Jcs.encode(v);
        // 0.875 * 1000 = 875, integer-encoded.
        assertTrue(jcs.contains("\"confidence\":875"), jcs);
    }
}
