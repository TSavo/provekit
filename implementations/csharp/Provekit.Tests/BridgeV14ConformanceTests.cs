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
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    private const string TargetCid =
        "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
    private const string TargetLayer = "rust-kit";
    private const string ProducedBy = "provekit-canonical-reference@v1.4";
    private const string ProducedAtAlt = "2026-05-03T00:00:00.000Z";

    // Expected JCS bytes from the conformance fixture
    private const string ExpectedJcs =
        "{\"envelope\":{\"declaredAt\":\"2026-05-03T00:00:00.000Z\",\"signature\":\"ed25519:RMYnQheAjTz7Ydq2yr1yl2Ramj/5G4eyhIb0DH1u3HKI7+95UAZnB3hEdgz0wqc+9BSe38SVTc1CmvyK8YVIBw==\",\"signer\":\"ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=\"},\"header\":{\"kind\":\"bridge\",\"name\":\"rust-canonical-bridge-fixture\",\"schemaVersion\":\"1\",\"sourceContractCid\":\"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\",\"sourceLayer\":\"rust-kit\",\"sourceSymbol\":\"parseInt\",\"target\":{\"cid\":\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"kind\":\"contract\"}},\"metadata\":{\"producedAt\":\"2026-05-03T00:00:00.000Z\",\"producedBy\":\"provekit-canonical-reference@v1.4\",\"targetLayer\":\"rust-kit\"}}";

    private const string ExpectedHash =
        "blake3-512:660ce98742d1f7ff326c994e4f6aba4d396d7fba0914db91a142c489e6d0901a7eff0ca206ce49bfa5b71eda289a138049fa8cf6461c5ef353703a78c0966cf2";

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
