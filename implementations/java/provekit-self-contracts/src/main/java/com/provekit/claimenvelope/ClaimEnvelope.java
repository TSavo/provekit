// SPDX-License-Identifier: Apache-2.0
//
// `mintContract` — build a signed claim envelope (the universal memento
// wrapper around a role-specific evidence body). Returns canonical bytes
// + CID.
//
// Java peer of implementations/rust/provekit-claim-envelope/src/lib.rs
// and implementations/csharp/Provekit.ClaimEnvelope/Mint.cs. v1.1.0
// hash widening: every hash is BLAKE3-512 (full 64-byte digest) prefixed
// with "blake3-512:". CIDs use the same form. NO truncation.
//
// Per-formula hashes (preHash/postHash/invHash) and propertyHash /
// bindingHash are DERIVED here from caller-supplied formula Values,
// never accepted from the caller. Validators recompute and reject
// mismatches.
//
// Scope vs. csharp peer: this Java port covers `mintContract`,
// `contractCid`, and `contractSetCid` — the load-bearing trio used by
// the rust orchestrator's `mint_from_ir_document` flow. Bridge and
// implication minting are tracked as follow-ups; they share the same
// {@link #mintInternal} substrate and can be added without disturbing
// the byte-equivalence already pinned here.

package com.provekit.claimenvelope;

import com.provekit.canonicalizer.Hash;
import com.provekit.canonicalizer.Jcs;
import com.provekit.canonicalizer.Value;
import com.provekit.proofenvelope.Sign;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;

public final class ClaimEnvelope {

    private ClaimEnvelope() {}

    // -----------------------------------------------------------------------
    // Schema CIDs (placeholders; mirrored byte-for-byte from rust/csharp)
    // -----------------------------------------------------------------------

    /** Placeholder schema CID for contract envelopes. */
    public static final String SCHEMA_CID_CONTRACT =
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c01";

    // -----------------------------------------------------------------------
    // Result type
    // -----------------------------------------------------------------------

    public static final class MintedEnvelope {
        public final byte[] canonicalBytes;
        public final String cid;

        MintedEnvelope(byte[] canonicalBytes, String cid) {
            this.canonicalBytes = canonicalBytes;
            this.cid = cid;
        }
    }

    // -----------------------------------------------------------------------
    // mintContract input
    // -----------------------------------------------------------------------

    public static final class MintContractArgs {
        public final String contractName;
        public final Value pre; // nullable
        public final Value post; // nullable
        public final Value inv; // nullable
        public final String outBinding;
        public final String producedBy;
        public final String producedAt;
        public final List<String> inputCids;
        public final Authoring authoring;
        public final byte[] signerSeed;

        private MintContractArgs(Builder b) {
            this.contractName = b.contractName;
            this.pre = b.pre;
            this.post = b.post;
            this.inv = b.inv;
            this.outBinding = b.outBinding;
            this.producedBy = b.producedBy;
            this.producedAt = b.producedAt;
            this.inputCids = Collections.unmodifiableList(new ArrayList<>(b.inputCids));
            this.authoring = b.authoring;
            this.signerSeed = b.signerSeed.clone();
        }

        public static Builder builder() { return new Builder(); }

        public static final class Builder {
            private String contractName;
            private Value pre;
            private Value post;
            private Value inv;
            private String outBinding = "out";
            private String producedBy;
            private String producedAt;
            private List<String> inputCids = new ArrayList<>();
            private Authoring authoring;
            private byte[] signerSeed;

            public Builder contractName(String v) { this.contractName = v; return this; }
            public Builder pre(Value v) { this.pre = v; return this; }
            public Builder post(Value v) { this.post = v; return this; }
            public Builder inv(Value v) { this.inv = v; return this; }
            public Builder outBinding(String v) { this.outBinding = v; return this; }
            public Builder producedBy(String v) { this.producedBy = v; return this; }
            public Builder producedAt(String v) { this.producedAt = v; return this; }
            public Builder inputCids(List<String> v) {
                this.inputCids = new ArrayList<>(v);
                return this;
            }
            public Builder authoring(Authoring v) { this.authoring = v; return this; }
            public Builder signerSeed(byte[] v) {
                if (v == null || v.length != 32) {
                    throw new IllegalArgumentException("signer seed must be 32 bytes");
                }
                this.signerSeed = v.clone();
                return this;
            }

            public MintContractArgs build() {
                if (contractName == null) throw new IllegalStateException("contractName required");
                if (producedBy == null) throw new IllegalStateException("producedBy required");
                if (producedAt == null) throw new IllegalStateException("producedAt required");
                if (authoring == null) throw new IllegalStateException("authoring required");
                if (signerSeed == null) throw new IllegalStateException("signerSeed required");
                return new MintContractArgs(this);
            }
        }
    }

    // -----------------------------------------------------------------------
    // CIDs (signer-independent)
    // -----------------------------------------------------------------------

    /**
     * Compute the signer-independent {@code contractCid} for a contract.
     *
     * <p>Per spec 2026-05-03-contract-cid-vs-attestation-cid.md §1:
     * <pre>
     *   contractCid = blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
     * </pre>
     * Two distinct signers attesting the same logical contract produce
     * the same contractCid. This is NOT the attestation CID (envelope
     * hash).
     */
    public static String contractCid(MintContractArgs args) {
        ArrayList<Map.Entry<String, Value>> entries = new ArrayList<>();
        entries.add(Map.entry("name", Value.ofString(args.contractName)));
        entries.add(Map.entry("outBinding", Value.ofString(args.outBinding)));
        if (args.pre != null) entries.add(Map.entry("pre", args.pre));
        if (args.post != null) entries.add(Map.entry("post", args.post));
        if (args.inv != null) entries.add(Map.entry("inv", args.inv));
        return hashValue(Value.ofObject(entries));
    }

    /**
     * Compute the {@code contractSetCid} from a list of signer-
     * independent contractCid strings.
     *
     * <p>Per spec 2026-05-03-contract-set-extension.md §1:
     * <pre>
     *   contractSetCid = blake3-512(JCS(&lt;sorted contractCids&gt;))
     * </pre>
     */
    public static String contractSetCid(Collection<String> contractCids) {
        ArrayList<String> sorted = new ArrayList<>(contractCids);
        // ASCII-sort for the contractCids (they are blake3-512:<hex> strings).
        sorted.sort(String::compareTo);
        ArrayList<Value> arr = new ArrayList<>(sorted.size());
        for (String s : sorted) arr.add(Value.ofString(s));
        return hashValue(Value.ofArray(arr));
    }

    // -----------------------------------------------------------------------
    // mintContract
    // -----------------------------------------------------------------------

    public static MintedEnvelope mintContract(MintContractArgs args) {
        if (args.pre == null && args.post == null && args.inv == null) {
            throw new IllegalArgumentException(
                "mintContract: at least one of pre/post/inv must be present");
        }
        if (args.outBinding == null || args.outBinding.isEmpty()) {
            throw new IllegalArgumentException("mintContract: outBinding must not be empty");
        }

        // Build evidence.body. Insertion order mirrors the C++/rust kits;
        // JCS sorts at emit time, so ordering is informational only.
        ArrayList<Map.Entry<String, Value>> body = new ArrayList<>();
        body.add(Map.entry("contractName", Value.ofString(args.contractName)));
        body.add(Map.entry("outBinding", Value.ofString(args.outBinding)));
        if (args.pre != null) {
            body.add(Map.entry("pre", args.pre));
            body.add(Map.entry("preHash", Value.ofString(hashValue(args.pre))));
        }
        if (args.post != null) {
            body.add(Map.entry("post", args.post));
            body.add(Map.entry("postHash", Value.ofString(hashValue(args.post))));
        }
        if (args.inv != null) {
            body.add(Map.entry("inv", args.inv));
            body.add(Map.entry("invHash", Value.ofString(hashValue(args.inv))));
        }
        body.add(Map.entry("authoring", args.authoring.toValue()));

        Value evidence = Value.ofObject(List.of(
            Map.entry("kind", Value.ofString("contract")),
            Map.entry("schema", Value.ofString(SCHEMA_CID_CONTRACT)),
            Map.entry("body", Value.ofObject(body))));

        // DERIVED:
        //   propertyHash = hash(canonical({pre?, post?, inv?, outBinding}))
        //   bindingHash  = hash(canonical({producerId, contractName, propertyHash}))
        ArrayList<Map.Entry<String, Value>> phEntries = new ArrayList<>();
        if (args.pre != null) phEntries.add(Map.entry("pre", args.pre));
        if (args.post != null) phEntries.add(Map.entry("post", args.post));
        if (args.inv != null) phEntries.add(Map.entry("inv", args.inv));
        phEntries.add(Map.entry("outBinding", Value.ofString(args.outBinding)));
        String propertyHash = hashValue(Value.ofObject(phEntries));

        Value bhObj = Value.ofObject(List.of(
            Map.entry("producerId", Value.ofString(args.producedBy)),
            Map.entry("contractName", Value.ofString(args.contractName)),
            Map.entry("propertyHash", Value.ofString(propertyHash))));
        String bindingHash = hashValue(bhObj);

        return mintInternal(
            bindingHash, propertyHash, "holds",
            args.producedBy, args.producedAt, args.inputCids,
            evidence, args.signerSeed);
    }

    // -----------------------------------------------------------------------
    // Internal: build wrapper, hash, sign, re-emit signed
    // -----------------------------------------------------------------------

    private static MintedEnvelope mintInternal(
            String bindingHash,
            String propertyHash,
            String verdict,
            String producedBy,
            String producedAt,
            List<String> inputCids,
            Value evidence,
            byte[] signerSeed) {

        // 1. Build the unsigned canonical envelope; hash it for the CID.
        Value unsignedV = buildEnvelopeForHashing(
            bindingHash, propertyHash, verdict, producedBy, producedAt, inputCids, evidence);
        byte[] unsignedBytes = Jcs.encodeUtf8(unsignedV);
        String cid = Hash.blake3_512(unsignedBytes);

        // 2. Sign the unsigned canonical bytes.
        String producerSig = Sign.ed25519SignString(signerSeed, unsignedBytes);

        // 3. Re-emit with cid + producerSignature appended; JCS re-sorts.
        ArrayList<Map.Entry<String, Value>> signedEntries =
            new ArrayList<>(unsignedV.asObject());
        signedEntries.add(Map.entry("cid", Value.ofString(cid)));
        signedEntries.add(Map.entry("producerSignature", Value.ofString(producerSig)));
        Value signedV = Value.ofObject(signedEntries);
        byte[] finalBytes = Jcs.encodeUtf8(signedV);
        return new MintedEnvelope(finalBytes, cid);
    }

    private static Value buildEnvelopeForHashing(
            String bindingHash,
            String propertyHash,
            String verdict,
            String producedBy,
            String producedAt,
            List<String> inputCids,
            Value evidence) {
        // Wrapper inputCids MUST be lex-sorted (spec §wrapper).
        ArrayList<String> sortedCids = new ArrayList<>(inputCids);
        sortedCids.sort(String::compareTo);
        ArrayList<Value> sortedArr = new ArrayList<>(sortedCids.size());
        for (String s : sortedCids) sortedArr.add(Value.ofString(s));

        return Value.ofObject(List.of(
            Map.entry("schemaVersion", Value.ofString("1")),
            Map.entry("bindingHash", Value.ofString(bindingHash)),
            Map.entry("propertyHash", Value.ofString(propertyHash)),
            Map.entry("verdict", Value.ofString(verdict)),
            Map.entry("producedBy", Value.ofString(producedBy)),
            Map.entry("producedAt", Value.ofString(producedAt)),
            Map.entry("inputCids", Value.ofArray(sortedArr)),
            Map.entry("evidence", evidence)));
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    private static String hashValue(Value v) {
        return Hash.blake3_512(Jcs.encodeUtf8(v));
    }

    /** Unused but kept symmetric with the csharp peer; trivial helper. */
    @SuppressWarnings("unused")
    private static String hashString(String s) {
        return Hash.blake3_512(s.getBytes(StandardCharsets.UTF_8));
    }
}
