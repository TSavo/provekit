// SPDX-License-Identifier: Apache-2.0
//
// `MintContract` / `MintBridge` / `MintImplication` — build a signed
// memento envelope (the universal claim-envelope wrapper around a
// role-specific evidence body). Each returns
// <see cref="MintedEnvelope"/> with canonical bytes + CID.
//
// Mirrors implementations/rust/provekit-claim-envelope/src/lib.rs 1:1
// with v1.1.0 hash widening: every hash is BLAKE3-512 (full 64-byte
// digest, hex-encoded) prefixed with "blake3-512:". CIDs use the same
// form. NO truncation.
//
// Per-formula hashes (preHash/postHash/invHash) and propertyHash /
// bindingHash are DERIVED here from caller-supplied formula Values,
// never accepted from the caller. Validators recompute and reject
// mismatches.

using Provekit.Canonicalizer;
using Provekit.IR;
using Provekit.ProofEnvelope;
using V = Provekit.Canonicalizer.Value;

namespace Provekit.ClaimEnvelope;

public sealed class MintedEnvelope
{
    public required byte[] CanonicalBytes { get; init; }
    public required string Cid { get; init; }
}

internal static class SchemaCids
{
    // Placeholder full-shape blake3-512 strings tagged with the role so
    // they don't collide. Mirrors the Rust peer's placeholders byte-for-
    // byte; replaced with real catalog CIDs once the catalog itself
    // lands on blake3-512.
    public const string Contract =
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c01";
    public const string Bridge =
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c03";
    public const string Implication =
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c08";
}

public sealed class MintContractArgs
{
    public required string ContractName { get; init; }
    public V? Pre { get; init; }
    public V? Post { get; init; }
    public V? Inv { get; init; }
    public string OutBinding { get; init; } = "out";
    public required string ProducedBy { get; init; }
    public required string ProducedAt { get; init; }
    public IReadOnlyList<string> InputCids { get; init; } = Array.Empty<string>();
    public required Authoring Authoring { get; init; }
    public required byte[] SignerSeed { get; init; }
}

public sealed class MintBridgeArgs
{
    public required string ProducedBy { get; init; }
    public required string ProducedAt { get; init; }
    public required string SourceSymbol { get; init; }
    public required string SourceLayer { get; init; }
    public required string TargetContractCid { get; init; }
    public required string TargetLayer { get; init; }
    public IReadOnlyList<string> IrArgSorts { get; init; } = Array.Empty<string>();
    public required string IrReturnSort { get; init; }
    public string Notes { get; init; } = "";
    public required byte[] SignerSeed { get; init; }
}

public sealed class MintImplicationArgs
{
    public required string ProducedBy { get; init; }
    public required string ProducedAt { get; init; }
    public required string AntecedentHash { get; init; }
    public required string ConsequentHash { get; init; }
    public required string AntecedentCid { get; init; }
    public required string ConsequentCid { get; init; }
    public string AntecedentSlot { get; init; } = "";
    public string ConsequentSlot { get; init; } = "";
    public string Prover { get; init; } = "";
    public long ProverRunMs { get; init; }
    public string SmtLibInput { get; init; } = "";
    public string ProofWitness { get; init; } = "";
    public required byte[] SignerSeed { get; init; }
}

public static class Mint
{
    private static string HashValue(V v) =>
        Hash.Blake3_512(Jcs.EncodeUtf8(v));

    /// <summary>
    /// Compute the signer-independent contractCid for a contract.
    ///
    /// Per spec 2026-05-03-contract-cid-vs-attestation-cid.md §1:
    ///   contractCid = blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
    ///
    /// Two distinct signers attesting the same logical contract produce the
    /// same contractCid. This is NOT the attestation CID (envelope hash).
    /// </summary>
    public static string ContractCid(MintContractArgs args)
    {
        var entries = new List<KeyValuePair<string, V>>
        {
            new("name", V.String(args.ContractName)),
            new("outBinding", V.String(args.OutBinding)),
        };
        if (args.Pre is not null) entries.Add(new("pre", args.Pre));
        if (args.Post is not null) entries.Add(new("post", args.Post));
        if (args.Inv is not null) entries.Add(new("inv", args.Inv));
        return HashValue(V.Object(entries));
    }

    /// <summary>
    /// Compute the contractSetCid from a list of signer-independent
    /// contractCid strings (each "blake3-512:&lt;128 hex&gt;").
    ///
    /// Per spec 2026-05-03-contract-set-extension.md §1:
    ///   contractSetCid = blake3-512(JCS(&lt;sorted contractCIDs&gt;))
    /// </summary>
    public static string ContractSetCid(IEnumerable<string> contractCids)
    {
        var sorted = contractCids.OrderBy(s => s, StringComparer.Ordinal).Select(V.String).ToArray();
        return HashValue(V.Array(sorted));
    }

    private static string HashString(string s) =>
        Hash.Blake3_512Utf8(s);

    private static V BuildEnvelopeForHashing(
        string bindingHash,
        string propertyHash,
        string verdict,
        string producedBy,
        string producedAt,
        IReadOnlyList<string> inputCids,
        V evidence)
    {
        // Wrapper ORDERING: inputCids MUST be lex-sorted (spec §wrapper).
        var sorted = inputCids.OrderBy(s => s, StringComparer.Ordinal).Select(V.String).ToArray();
        return V.Object(
            ("schemaVersion", V.String("1")),
            ("bindingHash", V.String(bindingHash)),
            ("propertyHash", V.String(propertyHash)),
            ("verdict", V.String(verdict)),
            ("producedBy", V.String(producedBy)),
            ("producedAt", V.String(producedAt)),
            ("inputCids", V.Array(sorted)),
            ("evidence", evidence)
        );
    }

    private static MintedEnvelope MintInternal(
        string bindingHash,
        string propertyHash,
        string verdict,
        string producedBy,
        string producedAt,
        IReadOnlyList<string> inputCids,
        V evidence,
        byte[] signerSeed)
    {
        // 1. Build the unsigned canonical envelope; hash it for the CID.
        var unsignedV = BuildEnvelopeForHashing(
            bindingHash, propertyHash, verdict, producedBy, producedAt, inputCids, evidence);
        var unsignedBytes = Jcs.EncodeUtf8(unsignedV);
        var cid = Hash.Blake3_512(unsignedBytes);
        // 2. Sign the unsigned canonical bytes.
        var producerSig = Sign.Ed25519SignString(signerSeed, unsignedBytes);

        // 3. Re-emit with cid + producerSignature appended; JCS re-sorts.
        var entries = new List<KeyValuePair<string, V>>(unsignedV.AsObject());
        entries.Add(new KeyValuePair<string, V>("cid", V.String(cid)));
        entries.Add(new KeyValuePair<string, V>("producerSignature", V.String(producerSig)));
        var signedV = V.Object(entries);
        var finalBytes = Jcs.EncodeUtf8(signedV);
        return new MintedEnvelope { CanonicalBytes = finalBytes, Cid = cid };
    }

    // -------------------------------------------------------------------
    // mint_contract
    // -------------------------------------------------------------------

    public static MintedEnvelope MintContract(MintContractArgs args)
    {
        if (args.Pre is null && args.Post is null && args.Inv is null)
        {
            throw new InvalidOperationException(
                "mint_contract: at least one of pre/post/inv must be present");
        }
        if (string.IsNullOrEmpty(args.OutBinding))
        {
            throw new InvalidOperationException("mint_contract: outBinding must not be empty");
        }

        // Build evidence.body. Insertion order mirrors C++/Rust kits.
        var body = new List<KeyValuePair<string, V>>
        {
            new("contractName", V.String(args.ContractName)),
            new("outBinding", V.String(args.OutBinding)),
        };
        if (args.Pre is not null)
        {
            body.Add(new("pre", args.Pre));
            body.Add(new("preHash", V.String(HashValue(args.Pre))));
        }
        if (args.Post is not null)
        {
            body.Add(new("post", args.Post));
            body.Add(new("postHash", V.String(HashValue(args.Post))));
        }
        if (args.Inv is not null)
        {
            body.Add(new("inv", args.Inv));
            body.Add(new("invHash", V.String(HashValue(args.Inv))));
        }
        body.Add(new("authoring", args.Authoring.ToValue()));

        var evidence = V.Object(
            ("kind", V.String("contract")),
            ("schema", V.String(SchemaCids.Contract)),
            ("body", V.Object(body))
        );

        // DERIVED:
        //   propertyHash = hash(canonical({pre?, post?, inv?, outBinding}))
        //   bindingHash  = hash(canonical({producerId, contractName, propertyHash}))
        var phEntries = new List<KeyValuePair<string, V>>();
        if (args.Pre is not null) phEntries.Add(new("pre", args.Pre));
        if (args.Post is not null) phEntries.Add(new("post", args.Post));
        if (args.Inv is not null) phEntries.Add(new("inv", args.Inv));
        phEntries.Add(new("outBinding", V.String(args.OutBinding)));
        var propertyHash = HashValue(V.Object(phEntries));

        var bhObj = V.Object(
            ("producerId", V.String(args.ProducedBy)),
            ("contractName", V.String(args.ContractName)),
            ("propertyHash", V.String(propertyHash))
        );
        var bindingHash = HashValue(bhObj);

        return MintInternal(
            bindingHash, propertyHash, "holds",
            args.ProducedBy, args.ProducedAt, args.InputCids,
            evidence, args.SignerSeed);
    }

    // -------------------------------------------------------------------
    // mint_bridge
    // -------------------------------------------------------------------

    public static MintedEnvelope MintBridge(MintBridgeArgs args)
    {
        var argSorts = args.IrArgSorts.Select(V.String).ToArray();
        var body = new List<KeyValuePair<string, V>>
        {
            new("sourceSymbol", V.String(args.SourceSymbol)),
            new("sourceLayer", V.String(args.SourceLayer)),
            new("targetContractCid", V.String(args.TargetContractCid)),
            new("targetLayer", V.String(args.TargetLayer)),
            new("irArgSorts", V.Array(argSorts)),
            new("irReturnSort", V.String(args.IrReturnSort)),
        };
        if (!string.IsNullOrEmpty(args.Notes))
        {
            body.Add(new("notes", V.String(args.Notes)));
        }

        var evidence = V.Object(
            ("kind", V.String("bridge")),
            ("schema", V.String(SchemaCids.Bridge)),
            ("body", V.Object(body))
        );

        // DERIVED:
        //   bindingHash  = hash(canonical({sourceLayer, sourceSymbol}))
        //   propertyHash = hash("bridge:" || sourceSymbol)
        var bhObj = V.Object(
            ("sourceLayer", V.String(args.SourceLayer)),
            ("sourceSymbol", V.String(args.SourceSymbol))
        );
        var bindingHash = HashValue(bhObj);
        var propertyHash = HashString($"bridge:{args.SourceSymbol}");

        return MintInternal(
            bindingHash, propertyHash, "holds",
            args.ProducedBy, args.ProducedAt, new[] { args.TargetContractCid },
            evidence, args.SignerSeed);
    }

    // -------------------------------------------------------------------
    // mint_bridge_v14 (v1.4 layered envelope/header/body, tagged-union target)
    //
    // Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1-R6
    // and substrate-layers-envelope-header-body.md §1-2.
    //
    // Canonical reference: rust/provekit-claim-envelope/src/lib.rs fn mint_bridge_v14.
    // -------------------------------------------------------------------

    public static MintedEnvelope MintBridgeV14(MintBridgeV14Args args)
    {
        // Build target value
        var targetEntries = new List<KeyValuePair<string, V>>
        {
            new("kind", V.String(args.Target.Kind)),
            new("cid", V.String(args.Target.Cid)),
        };
        var targetV = V.Object(targetEntries);

        // Build header (7 canonical fields per spec §1.R3)
        var header = V.Object(
            ("schemaVersion", V.String("1")),
            ("kind", V.String("bridge")),
            ("name", V.String(args.Name)),
            ("sourceSymbol", V.String(args.SourceSymbol)),
            ("sourceLayer", V.String(args.SourceLayer)),
            ("sourceContractCid", V.String(args.SourceContractCid)),
            ("target", targetV)
        );

        // Build metadata (only Some fields emitted)
        var metaEntries = new List<KeyValuePair<string, V>>();
        if (args.TargetWitnessCid is { } twc) metaEntries.Add(new("targetWitnessCid", V.String(twc)));
        if (args.TargetBinaryCid is { } tbc) metaEntries.Add(new("targetBinaryCid", V.String(tbc)));
        if (args.TargetLayer is { } tl) metaEntries.Add(new("targetLayer", V.String(tl)));
        if (args.TargetContractSetCid is { } tcs) metaEntries.Add(new("targetContractSetCid", V.String(tcs)));
        if (args.ProducedBy is { } pb) metaEntries.Add(new("producedBy", V.String(pb)));
        if (args.ProducedAt is { } pa) metaEntries.Add(new("producedAt", V.String(pa)));
        var metadata = V.Object(metaEntries);

        // Sign: JCS({header, metadata})
        var sigPayload = Jcs.EncodeUtf8(V.Object(
            ("header", header),
            ("metadata", metadata)
        ));
        var sig = Sign.Ed25519SignString(args.SignerSeed, sigPayload);

        // Build envelope (signer + declaredAt + signature). Construct
        // with the signature inline rather than mutating an existing
        // V.Object — V.Object's underlying list is read-only.
        var signerPubkey = Sign.Ed25519PubkeyString(args.SignerSeed);
        var envelope = V.Object(
            ("signer", V.String(signerPubkey)),
            ("declaredAt", V.String(args.DeclaredAt)),
            ("signature", V.String(sig))
        );

        // Full memento: {envelope, header, metadata}
        var memento = V.Object(
            ("envelope", envelope),
            ("header", header),
            ("metadata", metadata)
        );
        var canonical = Jcs.EncodeUtf8(memento);
        var cid = Hash.Blake3_512(canonical);

        return new MintedEnvelope { CanonicalBytes = canonical, Cid = cid };
    }

    // -------------------------------------------------------------------
    // mint_implication
    // -------------------------------------------------------------------

    public static MintedEnvelope MintImplication(MintImplicationArgs args)
    {
        var body = new List<KeyValuePair<string, V>>
        {
            new("antecedentHash", V.String(args.AntecedentHash)),
            new("consequentHash", V.String(args.ConsequentHash)),
            new("antecedentCid", V.String(args.AntecedentCid)),
            new("consequentCid", V.String(args.ConsequentCid)),
            new("antecedentSlot", V.String(args.AntecedentSlot)),
            new("consequentSlot", V.String(args.ConsequentSlot)),
            new("prover", V.String(args.Prover)),
            new("proverRunMs", V.Integer(args.ProverRunMs)),
        };
        if (!string.IsNullOrEmpty(args.SmtLibInput))
        {
            body.Add(new("smtLibInput", V.String(args.SmtLibInput)));
        }
        if (!string.IsNullOrEmpty(args.ProofWitness))
        {
            body.Add(new("proofWitness", V.String(args.ProofWitness)));
        }

        var evidence = V.Object(
            ("kind", V.String("implication")),
            ("schema", V.String(SchemaCids.Implication)),
            ("body", V.Object(body))
        );

        var bhObj = V.Object(
            ("antecedentHash", V.String(args.AntecedentHash)),
            ("consequentHash", V.String(args.ConsequentHash))
        );
        var bindingHash = HashValue(bhObj);
        var propertyHash = HashString(
            $"implication:{args.AntecedentHash}:{args.ConsequentHash}");

        var inputCids = new[] { args.AntecedentCid, args.ConsequentCid };
        return MintInternal(
            bindingHash, propertyHash, "holds",
            args.ProducedBy, args.ProducedAt, inputCids,
            evidence, args.SignerSeed);
    }
}
