// SPDX-License-Identifier: Apache-2.0
//
// {@code mintContract} / {@code mintBridge} / {@code mintImplication}
// build a signed memento in the v1.2 LAYERED shape introduced by
// `protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`:
//
//   { "envelope": {...}, "header": {...}, "metadata": {...} }
//
//   * envelope = { signer, declaredAt, signature }
//       The signature is computed over JCS({"header": header, "metadata": metadata}).
//       The envelope's CID (= attestation CID) is BLAKE3-512(JCS(envelope))
//       AFTER the signature has been embedded.
//
//   * header   = substrate-load-bearing data the verifier reads:
//                schemaVersion, kind, cid, plus kind-specific REQUIRED
//                fields and the derived hashes (bindingHash,
//                propertyHash, verdict, inputCids).
//
//   * metadata = everything else (authoring attribution, lifecycle
//                strings, derived per-formula hashes that are pure
//                tooling convenience). Opaque to the substrate verifier;
//                signed transitively via the envelope.
//
// Mirrors implementations/rust/provekit-claim-envelope/src/lib.rs and
// implementations/csharp/Provekit.ClaimEnvelope/Mint.cs 1:1.

package com.provekit.claimenvelope;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.TreeMap;

import com.provekit.claimenvelope.Jcs.Value;

public final class ClaimEnvelope {

    public static final String LAYERED_SCHEMA_VERSION = "2";

    private ClaimEnvelope() {}

    public static final class MintedEnvelope {
        public final byte[] canonicalBytes;
        /** Attestation CID: BLAKE3-512(JCS(envelope-with-signature)). */
        public final String cid;
        /** Content CID for contracts (signer-independent); empty for bridges/implications. */
        public final String contractCid;

        MintedEnvelope(byte[] canonicalBytes, String cid, String contractCid) {
            this.canonicalBytes = canonicalBytes;
            this.cid = cid;
            this.contractCid = contractCid;
        }
    }

    // =========================================================================
    // Authoring (typed union mirroring the rust/csharp peers)
    // =========================================================================

    public static abstract sealed class Authoring
            permits Authoring.KitAuthor, Authoring.Lift, Authoring.Llm {

        public static final class KitAuthor extends Authoring {
            public final String author;
            public final String note; // nullable; empty/null both omitted
            public KitAuthor(String author, String note) {
                this.author = Objects.requireNonNull(author);
                this.note = note;
            }
        }

        public static final class Lift extends Authoring {
            public final String lifter;
            public final String evidence;
            public final String sourceCid; // nullable
            public Lift(String lifter, String evidence, String sourceCid) {
                this.lifter = Objects.requireNonNull(lifter);
                this.evidence = Objects.requireNonNull(evidence);
                this.sourceCid = sourceCid;
            }
        }

        public static final class Llm extends Authoring {
            public final String llm;
            public final String llmVersion;
            public final String promptCid;
            public final double confidence;
            public final String rationale; // nullable
            public Llm(String llm, String llmVersion, String promptCid, double confidence, String rationale) {
                this.llm = Objects.requireNonNull(llm);
                this.llmVersion = Objects.requireNonNull(llmVersion);
                this.promptCid = Objects.requireNonNull(promptCid);
                this.confidence = confidence;
                this.rationale = rationale;
            }
        }
    }

    private static Value authoringToValue(Authoring a) {
        if (a instanceof Authoring.KitAuthor ka) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("producerKind", Value.string("kit-author"));
            m.put("author", Value.string(ka.author));
            if (ka.note != null && !ka.note.isEmpty()) {
                m.put("note", Value.string(ka.note));
            }
            return Value.object(m);
        }
        if (a instanceof Authoring.Lift l) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("producerKind", Value.string("lift"));
            m.put("lifter", Value.string(l.lifter));
            m.put("evidence", Value.string(l.evidence));
            if (l.sourceCid != null && !l.sourceCid.isEmpty()) {
                m.put("sourceCid", Value.string(l.sourceCid));
            }
            return Value.object(m);
        }
        if (a instanceof Authoring.Llm ll) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("producerKind", Value.string("llm"));
            m.put("llm", Value.string(ll.llm));
            m.put("llmVersion", Value.string(ll.llmVersion));
            m.put("promptCid", Value.string(ll.promptCid));
            // Confidence is rendered as integer(confidence * 1000), matching
            // the rust peer's wire format.
            m.put("confidence", Value.integer((long) (ll.confidence * 1000.0)));
            if (ll.rationale != null && !ll.rationale.isEmpty()) {
                m.put("rationale", Value.string(ll.rationale));
            }
            return Value.object(m);
        }
        throw new IllegalStateException("unknown Authoring variant: " + a.getClass());
    }

    // =========================================================================
    // Common helpers
    // =========================================================================

    private static String hashValue(Value v) {
        return Blake3.blake3_512(Jcs.encodeUtf8(v));
    }

    private static String hashString(String s) {
        return Blake3.blake3_512(s.getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    /**
     * Build the JCS-canonical bytes of {@code {"header": header, "metadata": metadata}}.
     * This is the message the envelope's Ed25519 signature covers.
     */
    private static byte[] signingBytes(Value header, Value metadata) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("header", header);
        m.put("metadata", metadata);
        return Jcs.encodeUtf8(Value.object(m));
    }

    private static Value buildHeader(String kind, String headerCid, LinkedHashMap<String, Value> kindSpecific) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("schemaVersion", Value.string(LAYERED_SCHEMA_VERSION));
        m.put("kind", Value.string(kind));
        m.put("cid", Value.string(headerCid));
        m.putAll(kindSpecific);
        return Value.object(m);
    }

    /**
     * Assemble a layered memento, sign it, and compute the attestation CID
     * (= BLAKE3-512(JCS(envelope-with-signature))).
     */
    private static MintedEnvelope assembleLayered(Value header,
                                                  Value metadata,
                                                  String declaredAt,
                                                  byte[] signerSeed,
                                                  String contentCid) {
        String signer = Ed25519.pubkeyString(signerSeed);
        byte[] signingMsg = signingBytes(header, metadata);
        String signature = Ed25519.signString(signerSeed, signingMsg);

        LinkedHashMap<String, Value> envEntries = new LinkedHashMap<>();
        envEntries.put("signer", Value.string(signer));
        envEntries.put("declaredAt", Value.string(declaredAt));
        envEntries.put("signature", Value.string(signature));
        Value envelope = Value.object(envEntries);

        byte[] envelopeJcs = Jcs.encodeUtf8(envelope);
        String attestationCid = Blake3.blake3_512(envelopeJcs);

        LinkedHashMap<String, Value> mementoEntries = new LinkedHashMap<>();
        mementoEntries.put("envelope", envelope);
        mementoEntries.put("header", header);
        mementoEntries.put("metadata", metadata);
        Value memento = Value.object(mementoEntries);

        byte[] mementoJcs = Jcs.encodeUtf8(memento);
        return new MintedEnvelope(mementoJcs, attestationCid, contentCid);
    }

    // =========================================================================
    // Contracts
    // =========================================================================

    public static final class MintContractArgs {
        public String contractName;
        public Value pre;          // nullable
        public Value post;         // nullable
        public Value inv;          // nullable
        public String outBinding = "out";
        public String producedBy;
        public String producedAt;
        public List<String> inputCids = List.of();
        public Authoring authoring;
        public byte[] signerSeed;
    }

    /** Compute the signer-independent contract CID for a logical contract. */
    public static String contractCid(MintContractArgs args) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("name", Value.string(args.contractName));
        m.put("outBinding", Value.string(args.outBinding));
        if (args.pre != null) m.put("pre", args.pre);
        if (args.post != null) m.put("post", args.post);
        if (args.inv != null) m.put("inv", args.inv);
        return Blake3.blake3_512(Jcs.encodeUtf8(Value.object(m)));
    }

    /** Compute the contract-set CID from a list of contract CIDs (sort order independent). */
    public static String computeContractSetCid(List<String> contractCids) {
        List<String> sorted = new ArrayList<>(contractCids);
        Collections.sort(sorted);
        List<Value> arr = new ArrayList<>(sorted.size());
        for (String c : sorted) arr.add(Value.string(c));
        return Blake3.blake3_512(Jcs.encodeUtf8(Value.array(arr)));
    }

    public static MintedEnvelope mintContract(MintContractArgs args) {
        if (args.pre == null && args.post == null && args.inv == null) {
            throw new IllegalArgumentException("mintContract: at least one of pre/post/inv must be present");
        }
        if (args.outBinding == null || args.outBinding.isEmpty()) {
            throw new IllegalArgumentException("mintContract: outBinding must not be empty");
        }

        LinkedHashMap<String, Value> phMap = new LinkedHashMap<>();
        if (args.pre != null) phMap.put("pre", args.pre);
        if (args.post != null) phMap.put("post", args.post);
        if (args.inv != null) phMap.put("inv", args.inv);
        phMap.put("outBinding", Value.string(args.outBinding));
        String propertyHash = hashValue(Value.object(phMap));

        LinkedHashMap<String, Value> bhMap = new LinkedHashMap<>();
        bhMap.put("producerId", Value.string(args.producedBy));
        bhMap.put("contractName", Value.string(args.contractName));
        bhMap.put("propertyHash", Value.string(propertyHash));
        String bindingHash = hashValue(Value.object(bhMap));

        String headerCid = contractCid(args);

        LinkedHashMap<String, Value> kindSpecific = new LinkedHashMap<>();
        kindSpecific.put("name", Value.string(args.contractName));
        kindSpecific.put("outBinding", Value.string(args.outBinding));
        if (args.pre != null) kindSpecific.put("pre", args.pre);
        if (args.post != null) kindSpecific.put("post", args.post);
        if (args.inv != null) kindSpecific.put("inv", args.inv);
        kindSpecific.put("verdict", Value.string("holds"));
        kindSpecific.put("bindingHash", Value.string(bindingHash));
        kindSpecific.put("propertyHash", Value.string(propertyHash));
        List<String> sortedInputs = new ArrayList<>(args.inputCids);
        Collections.sort(sortedInputs);
        List<Value> inputs = new ArrayList<>(sortedInputs.size());
        for (String s : sortedInputs) inputs.add(Value.string(s));
        kindSpecific.put("inputCids", Value.array(inputs));

        Value header = buildHeader("contract", headerCid, kindSpecific);

        LinkedHashMap<String, Value> meta = new LinkedHashMap<>();
        meta.put("authoring", authoringToValue(args.authoring));
        meta.put("producedBy", Value.string(args.producedBy));
        meta.put("producedAt", Value.string(args.producedAt));
        if (args.pre != null) meta.put("preHash", Value.string(hashValue(args.pre)));
        if (args.post != null) meta.put("postHash", Value.string(hashValue(args.post)));
        if (args.inv != null) meta.put("invHash", Value.string(hashValue(args.inv)));
        Value metadata = Value.object(meta);

        return assembleLayered(header, metadata, args.producedAt, args.signerSeed, headerCid);
    }

    // =========================================================================
    // Bridges
    // =========================================================================

    public static final class MintBridgeArgs {
        public String producedBy;
        public String producedAt;
        public String sourceSymbol;
        public String sourceLayer;
        public String targetContractCid;
        public String targetLayer;
        public List<String> irArgSorts = List.of();
        public String irReturnSort;
        public String notes = "";
        public byte[] signerSeed;
    }

    private static String bridgeContentCid(MintBridgeArgs args) {
        List<Value> argSorts = new ArrayList<>(args.irArgSorts.size());
        for (String s : args.irArgSorts) argSorts.add(Value.string(s));
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("sourceSymbol", Value.string(args.sourceSymbol));
        m.put("sourceLayer", Value.string(args.sourceLayer));
        m.put("targetContractCid", Value.string(args.targetContractCid));
        m.put("targetLayer", Value.string(args.targetLayer));
        m.put("irArgSorts", Value.array(argSorts));
        m.put("irReturnSort", Value.string(args.irReturnSort));
        return Blake3.blake3_512(Jcs.encodeUtf8(Value.object(m)));
    }

    public static MintedEnvelope mintBridge(MintBridgeArgs args) {
        List<Value> argSorts = new ArrayList<>(args.irArgSorts.size());
        for (String s : args.irArgSorts) argSorts.add(Value.string(s));

        LinkedHashMap<String, Value> bh = new LinkedHashMap<>();
        bh.put("sourceLayer", Value.string(args.sourceLayer));
        bh.put("sourceSymbol", Value.string(args.sourceSymbol));
        String bindingHash = hashValue(Value.object(bh));
        String propertyHash = hashString("bridge:" + args.sourceSymbol);

        String headerCid = bridgeContentCid(args);
        LinkedHashMap<String, Value> kindSpecific = new LinkedHashMap<>();
        kindSpecific.put("sourceSymbol", Value.string(args.sourceSymbol));
        kindSpecific.put("sourceLayer", Value.string(args.sourceLayer));
        kindSpecific.put("targetContractCid", Value.string(args.targetContractCid));
        kindSpecific.put("targetLayer", Value.string(args.targetLayer));
        kindSpecific.put("irArgSorts", Value.array(argSorts));
        kindSpecific.put("irReturnSort", Value.string(args.irReturnSort));
        kindSpecific.put("verdict", Value.string("holds"));
        kindSpecific.put("bindingHash", Value.string(bindingHash));
        kindSpecific.put("propertyHash", Value.string(propertyHash));
        List<Value> inputs = new ArrayList<>(1);
        inputs.add(Value.string(args.targetContractCid));
        kindSpecific.put("inputCids", Value.array(inputs));

        Value header = buildHeader("bridge", headerCid, kindSpecific);

        LinkedHashMap<String, Value> meta = new LinkedHashMap<>();
        meta.put("producedBy", Value.string(args.producedBy));
        meta.put("producedAt", Value.string(args.producedAt));
        if (args.notes != null && !args.notes.isEmpty()) {
            meta.put("notes", Value.string(args.notes));
        }
        Value metadata = Value.object(meta);

        return assembleLayered(header, metadata, args.producedAt, args.signerSeed, "");
    }

    // =========================================================================
    // mintBridgeV14 (v1.4 BridgeDeclaration, layered envelope/header/body)
    // =========================================================================
    //
    // Source of truth for the wire shape:
    //   protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1-R6
    //   protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md §1, §2
    //   protocol/provekit-ir.cddl  BridgeDeclarationV14
    //
    // Mirrors implementations/rust/provekit-claim-envelope/src/lib.rs::mint_bridge_v14
    // 1:1. Differences from `mintBridge` (above):
    //   1. Header carries the contract-axis claim only. Witness/binary/
    //      target-layer axes move to the metadata block. (spec §1.R3)
    //   2. `target` is a tagged-union object {kind, cid}, not flat
    //      `targetContractCid` plus `targetLayer`. (spec §1.R1)
    //   3. `schemaVersion` is `"1"` (the v1.4-layered schema version),
    //      distinct from the v1.2 layered shape's `"2"`.
    //   4. Metadata fields that are unknown at mint time are OMITTED from
    //      the JCS bytes. They are NOT emitted as `null` and NOT emitted
    //      with placeholder strings. (spec §1.R2)
    //   5. Header has exactly seven fields (no `cid`, no derived
    //      `bindingHash`/`propertyHash`/`inputCids` -- those live in the
    //      v1.2 richer `mintBridge` shape and do NOT appear here).
    //
    // Co-existence with `mintBridge`:
    //   The v1.2-layered `mintBridge` (schemaVersion="2", richer header)
    //   remains the active path for the existing kit infrastructure. The
    //   v1.4 path is the canonical reference for cross-kit byte-equality
    //   per the substrate-layers / target-dimensionality specs. Both
    //   shapes coexist; v1.1 historical mementos remain valid forever
    //   (spec §4) and are never re-signed.

    /** Schema version stamp on v1.4 layered bridge headers. */
    public static final String BRIDGE_V14_SCHEMA_VERSION = "1";

    /**
     * Tagged-union target axis per
     * {@code 2026-05-03-bridge-target-dimensionality.md} §1.R1.
     *
     * <p>Implementations MUST emit exactly one variant. Implementations MUST NOT
     * emit a bare string for {@code target}; the substrate verifier rejects
     * stringified placeholders (spec §1.R2).
     */
    private static final java.util.regex.Pattern VALID_CID_PATTERN =
            java.util.regex.Pattern.compile("^blake3-512:[0-9a-f]{128}$");

    /** Validate that the given value is a well-formed CID and is not a
     *  placeholder string (spec §1.R2).  Throws {@link IllegalArgumentException}
     *  with a message naming the offending field + value if validation fails. */
    private static String requireValidCid(String value, String fieldName) {
        Objects.requireNonNull(value, fieldName);
        if (value.isEmpty()) {
            throw new IllegalArgumentException(
                    fieldName + " must not be empty; got empty string");
        }
        if (value.startsWith("pending-")) {
            throw new IllegalArgumentException(
                    fieldName + " must not be a placeholder string (pending-*:); got: " + value);
        }
        if (value.startsWith("deferred:")) {
            throw new IllegalArgumentException(
                    fieldName + " must not be a placeholder string (deferred:*); got: " + value);
        }
        if (!VALID_CID_PATTERN.matcher(value).matches()) {
            throw new IllegalArgumentException(
                    fieldName + " must match canonical CID grammar ^blake3-512:[0-9a-f]{128}$; got: " + value);
        }
        return value;
    }

    private static void requireNonEmpty(String value, String fieldName) {
        Objects.requireNonNull(value, fieldName);
        if (value.isEmpty()) {
            throw new IllegalArgumentException(
                    fieldName + " must not be empty in mintBridgeV14 args");
        }
    }

    public static abstract sealed class BridgeTargetV14
            permits BridgeTargetV14.Contract, BridgeTargetV14.ContractSet {

        public abstract String cid();
        public abstract String kind();

        /** {@code { "kind": "contract", "cid": "<contractCid>" }} */
        public static final class Contract extends BridgeTargetV14 {
            public final String cid;
            public Contract(String cid) {
                this.cid = requireValidCid(cid, "cid");
            }
            @Override public String cid() { return cid; }
            @Override public String kind() { return "contract"; }
        }

        /** {@code { "kind": "contractSet", "cid": "<contractSetCid>" }} */
        public static final class ContractSet extends BridgeTargetV14 {
            public final String cid;
            public ContractSet(String cid) {
                this.cid = requireValidCid(cid, "cid");
            }
            @Override public String cid() { return cid; }
            @Override public String kind() { return "contractSet"; }
        }

        Value toValue() {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string(kind()));
            m.put("cid", Value.string(cid()));
            return Value.object(m);
        }
    }

    /**
     * Inputs for {@link #mintBridgeV14(MintBridgeV14Args)}.
     *
     * <p>Optional metadata-axis fields are nullable {@code String}. {@code null}
     * means the field is OMITTED from the JCS bytes. Empty strings ARE distinct
     * from {@code null} and would be emitted as {@code ""}; callers SHOULD pass
     * {@code null} when the axis is unknown, to satisfy spec §1.R2.
     */
    public static final class MintBridgeV14Args {
        // ---- header (substrate-verified) ----
        public String name;
        public String sourceSymbol;
        public String sourceLayer;
        public String sourceContractCid;
        public BridgeTargetV14 target;

        // ---- metadata (optional, opaque to substrate) ----
        public String targetWitnessCid;       // nullable -> OMIT
        public String targetBinaryCid;        // nullable -> OMIT
        public String targetLayer;            // nullable -> OMIT
        public String targetContractSetCid;   // nullable -> OMIT
        public String producedBy;             // nullable -> OMIT
        public String producedAt;             // nullable -> OMIT

        // ---- envelope inputs ----
        /** RFC 3339 UTC timestamp for {@code envelope.declaredAt}. */
        public String declaredAt;
        public byte[] signerSeed;
    }

    /**
     * Build the v1.4 header object exactly as specified in
     * {@code 2026-05-03-bridge-target-dimensionality.md} §1.R3.
     *
     * <p>Inserts the seven canonical fields (schemaVersion, kind, name,
     * sourceSymbol, sourceLayer, sourceContractCid, target). The JCS encoder
     * sorts keys at emit time, so insertion order is just for readability.
     * The v1.4 header has NO {@code cid} field -- using the generic
     * {@link #buildHeader} helper here would inject one and break
     * byte-equivalence with the rust canonical reference.
     */
    private static Value buildBridgeHeaderV14(MintBridgeV14Args args) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("schemaVersion", Value.string(BRIDGE_V14_SCHEMA_VERSION));
        m.put("kind", Value.string("bridge"));
        m.put("name", Value.string(args.name));
        m.put("sourceSymbol", Value.string(args.sourceSymbol));
        m.put("sourceLayer", Value.string(args.sourceLayer));
        m.put("sourceContractCid", Value.string(args.sourceContractCid));
        m.put("target", args.target.toValue());
        return Value.object(m);
    }

    /**
     * Build the v1.4 metadata object. Only non-null fields are emitted;
     * null fields are OMITTED from the JCS bytes per spec §1.R2.
     */
    private static Value buildBridgeMetadataV14(MintBridgeV14Args args) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        if (args.targetWitnessCid != null) {
            m.put("targetWitnessCid", Value.string(args.targetWitnessCid));
        }
        if (args.targetBinaryCid != null) {
            m.put("targetBinaryCid", Value.string(args.targetBinaryCid));
        }
        if (args.targetLayer != null) {
            m.put("targetLayer", Value.string(args.targetLayer));
        }
        if (args.targetContractSetCid != null) {
            m.put("targetContractSetCid", Value.string(args.targetContractSetCid));
        }
        if (args.producedBy != null) {
            m.put("producedBy", Value.string(args.producedBy));
        }
        if (args.producedAt != null) {
            m.put("producedAt", Value.string(args.producedAt));
        }
        return Value.object(m);
    }

    /**
     * Mint a v1.4 BridgeDeclaration in the layered envelope/header/body
     * shape. The returned {@link MintedEnvelope} carries the JCS-canonical
     * bytes of the full memento and the attestation CID
     * (= BLAKE3-512(JCS(envelope))).
     *
     * <p>{@code contractCid} on the returned envelope is the empty string;
     * bridges have no signer-independent contract CID (only contracts do).
     */
    public static MintedEnvelope mintBridgeV14(MintBridgeV14Args args) {
        Objects.requireNonNull(args.target, "args.target");
        Objects.requireNonNull(args.declaredAt, "args.declaredAt");
        Objects.requireNonNull(args.signerSeed, "args.signerSeed");
        requireNonEmpty(args.name, "args.name");
        requireNonEmpty(args.sourceSymbol, "args.sourceSymbol");
        requireNonEmpty(args.sourceLayer, "args.sourceLayer");
        requireValidCid(args.sourceContractCid, "args.sourceContractCid");
        Value header = buildBridgeHeaderV14(args);
        Value metadata = buildBridgeMetadataV14(args);
        return assembleLayered(header, metadata, args.declaredAt, args.signerSeed, "");
    }

    // =========================================================================
    // Implications
    // =========================================================================

    public static final class MintImplicationArgs {
        public String producedBy;
        public String producedAt;
        public String antecedentHash;
        public String consequentHash;
        public String antecedentCid;
        public String consequentCid;
        public String antecedentSlot = "";
        public String consequentSlot = "";
        public String prover = "";
        public long proverRunMs;
        public String smtLibInput = "";
        public String proofWitness = "";
        public byte[] signerSeed;
    }

    private static String implicationContentCid(MintImplicationArgs args) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("antecedentHash", Value.string(args.antecedentHash));
        m.put("consequentHash", Value.string(args.consequentHash));
        m.put("antecedentCid", Value.string(args.antecedentCid));
        m.put("consequentCid", Value.string(args.consequentCid));
        m.put("antecedentSlot", Value.string(args.antecedentSlot));
        m.put("consequentSlot", Value.string(args.consequentSlot));
        return Blake3.blake3_512(Jcs.encodeUtf8(Value.object(m)));
    }

    public static MintedEnvelope mintImplication(MintImplicationArgs args) {
        LinkedHashMap<String, Value> bh = new LinkedHashMap<>();
        bh.put("antecedentHash", Value.string(args.antecedentHash));
        bh.put("consequentHash", Value.string(args.consequentHash));
        String bindingHash = hashValue(Value.object(bh));
        String propertyHash = hashString("implication:" + args.antecedentHash + ":" + args.consequentHash);

        String headerCid = implicationContentCid(args);
        List<String> inputCids = new ArrayList<>();
        inputCids.add(args.antecedentCid);
        inputCids.add(args.consequentCid);
        Collections.sort(inputCids);
        List<Value> inputArr = new ArrayList<>(inputCids.size());
        for (String s : inputCids) inputArr.add(Value.string(s));

        LinkedHashMap<String, Value> kindSpecific = new LinkedHashMap<>();
        kindSpecific.put("antecedentHash", Value.string(args.antecedentHash));
        kindSpecific.put("consequentHash", Value.string(args.consequentHash));
        kindSpecific.put("antecedentCid", Value.string(args.antecedentCid));
        kindSpecific.put("consequentCid", Value.string(args.consequentCid));
        kindSpecific.put("antecedentSlot", Value.string(args.antecedentSlot));
        kindSpecific.put("consequentSlot", Value.string(args.consequentSlot));
        kindSpecific.put("verdict", Value.string("holds"));
        kindSpecific.put("bindingHash", Value.string(bindingHash));
        kindSpecific.put("propertyHash", Value.string(propertyHash));
        kindSpecific.put("inputCids", Value.array(inputArr));

        Value header = buildHeader("implication", headerCid, kindSpecific);

        LinkedHashMap<String, Value> meta = new LinkedHashMap<>();
        meta.put("producedBy", Value.string(args.producedBy));
        meta.put("producedAt", Value.string(args.producedAt));
        meta.put("prover", Value.string(args.prover));
        meta.put("proverRunMs", Value.integer(args.proverRunMs));
        if (args.smtLibInput != null && !args.smtLibInput.isEmpty()) {
            meta.put("smtLibInput", Value.string(args.smtLibInput));
        }
        if (args.proofWitness != null && !args.proofWitness.isEmpty()) {
            meta.put("proofWitness", Value.string(args.proofWitness));
        }
        Value metadata = Value.object(meta);

        return assembleLayered(header, metadata, args.producedAt, args.signerSeed, "");
    }
}
