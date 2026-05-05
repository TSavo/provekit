// SPDX-License-Identifier: Apache-2.0
//
// Bridge IR v1.4 — C# byte-equivalence + round-trip conformance test.
// Pins against the canonical bridge_decl_v1_4 fixture in
// conformance/fixtures.toml. Mirrors rust's
// provekit-claim-envelope/tests/bridge_v14_roundtrip.rs.

using System.Text.Json;
using Xunit;
using Provekit.IR;
using Provekit.ClaimEnvelope;

namespace Provekit.Tests;

public class BridgeV14ConformanceTests
{
    private static readonly byte[] FoundationSeed = Enumerable.Repeat((byte)0x42, 32).ToArray();

    // ── Golden values from conformance/fixtures.toml bridge_decl_v1_4 ──

    private const string DeclaredAt = "2026-05-03T00:00:00.000Z";
    private const string Name = "rust-canonical-bridge-fixture";
    private const string SourceSymbol = "parseInt";
    private const string SourceLayer = "rust-kit";
    private const string SourceContractCid =
        "blake3-512:source0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    private const string TargetCid =
        "blake3-512:target0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    private const string TargetLayer = "rust-kit";
    private const string ProducedBy = "provekit-canonical-reference@v1.4";
    private const string ProducedAtAlt = "2026-05-03T00:00:00.000Z";

    // Expected JCS bytes from the conformance fixture
    private const string ExpectedJcs =
        "{\"envelope\":{\"declaredAt\":\"2026-05-03T00:00:00.000Z\",\"signature\":\"ed25519:GghyfAgvP5MtRcKjCBTvOf2qRqG13WboOLkZzkSbEbtNxqT+eDMcEup+RJWDOGBuhaBAR4jTPfM2w09iZsTuAw==\",\"signer\":\"ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=\"},\"header\":{\"kind\":\"bridge\",\"name\":\"rust-canonical-bridge-fixture\",\"schemaVersion\":\"1\",\"sourceContractCid\":\"blake3-512:source0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\",\"sourceLayer\":\"rust-kit\",\"sourceSymbol\":\"parseInt\",\"target\":{\"cid\":\"blake3-512:target0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\",\"kind\":\"contract\"}},\"metadata\":{\"producedAt\":\"2026-05-03T00:00:00.000Z\",\"producedBy\":\"provekit-canonical-reference@v1.4\",\"targetLayer\":\"rust-kit\"}}";

    private const string ExpectedHash =
        "blake3-512:270e867a46317f3c92a9af57d6aefe292f9a30a61149c1b7e22eb5500b203993ae029bce5e69dc6818ae0b2657d7960dac99b98c301c89050491c9d9c1852059";

    [Fact]
    public void RoundTrip_ProducesByteIdenticalJcs()
    {
        var target = new BridgeTargetV14.Contract(TargetCid);

        var args = new MintBridgeV14Args
        {
            Name = Name,
            SourceSymbol = SourceSymbol,
            SourceLayer = SourceLayer,
            SourceContractCid = SourceContractCid,
            Target = target,
            TargetLayer = TargetLayer,
            ProducedBy = ProducedBy,
            ProducedAt = ProducedAtAlt,
            DeclaredAt = DeclaredAt,
            SignerSeed = FoundationSeed,
        };

        var minted = Mint.MintBridgeV14(args);
        var jcsStr = System.Text.Encoding.UTF8.GetString(minted.CanonicalBytes);

        // JCS canonical bytes are byte-deterministic (RFC 8785), so the
        // string comparison is the right invariant here. JsonElement
        // value-equality is NOT — `JsonElement.Equals` falls back to
        // ValueType bitwise comparison on the struct, which is never
        // equal across two distinct JsonDocuments even when their
        // serialized JSON is byte-identical.
        Assert.Equal(ExpectedJcs, jcsStr);

        // BLAKE3-512 hash must match pinned golden. `minted.Cid`
        // already carries the `blake3-512:` self-identifying prefix
        // per the canonicalizer's hash output convention (matches the
        // Rust kit's contract).
        Assert.Equal(ExpectedHash, minted.Cid);
    }

    [Fact]
    public void TargetContract_VariantIsCorrect()
    {
        var t = new BridgeTargetV14.Contract("blake3-512:abc");
        Assert.Equal("contract", t.Kind);
    }

    [Fact]
    public void TargetContractSet_VariantIsCorrect()
    {
        var t = new BridgeTargetV14.ContractSet("blake3-512:def");
        Assert.Equal("contractSet", t.Kind);
    }

    [Fact]
    public void Metadata_NoneFields_Omitted()
    {
        var m = new BridgeMetadataV14(TargetLayer: "rust-kit");
        // Indirect test: only metadata with non-null fields should serialize
        Assert.Null(m.TargetWitnessCid);
        Assert.Null(m.TargetBinaryCid);
        Assert.NotNull(m.TargetLayer);
    }

    [Fact]
    public void Header_HasSevenCanonicalFields()
    {
        var t = new BridgeTargetV14.Contract(TargetCid);
        var h = new BridgeHeaderV14("test", "foo", "rust-kit", SourceContractCid, t);

        Assert.Equal("1", h.SchemaVersion);
        Assert.Equal("bridge", h.Kind);
        Assert.Equal("test", h.Name);
        Assert.Equal("foo", h.SourceSymbol);
        Assert.Equal("rust-kit", h.SourceLayer);
        Assert.Equal(SourceContractCid, h.SourceContractCid);
        Assert.Equal(t, h.Target);
    }
}
