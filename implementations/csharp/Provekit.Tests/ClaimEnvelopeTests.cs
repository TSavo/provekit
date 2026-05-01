// SPDX-License-Identifier: Apache-2.0
//
// Mint-flow tests for Provekit.ClaimEnvelope. Mirrors the Rust peer's
// integration tests: builds a contract memento end-to-end, asserts the
// CID is a full self-identifying blake3-512 string, and asserts the
// canonical bytes contain the spec-required fields.

using Provekit.ClaimEnvelope;
using V = Provekit.Canonicalizer.Value;
using Xunit;

namespace Provekit.Tests;

public class ClaimEnvelopeTests
{
    private static byte[] DummySeed() => Enumerable.Repeat((byte)0x42, 32).ToArray();

    [Fact]
    public void EmptyContract_Rejected()
    {
        var args = new MintContractArgs
        {
            ContractName = "x",
            ProducedBy = "test",
            ProducedAt = "2026-04-30T00:00:00.000Z",
            Authoring = new Authoring.KitAuthor("test"),
            SignerSeed = DummySeed(),
        };
        Assert.Throws<InvalidOperationException>(() => Mint.MintContract(args));
    }

    [Fact]
    public void ContractCid_IsBlake3_512_FullForm()
    {
        var pre = V.Object(
            ("kind", V.String("atomic")),
            ("name", V.String(">")),
            ("args", V.Array(
                V.Object(
                    ("kind", V.String("var")),
                    ("name", V.String("n"))
                ),
                V.Object(
                    ("kind", V.String("const")),
                    ("value", V.Integer(0)),
                    ("sort", V.Object(
                        ("kind", V.String("primitive")),
                        ("name", V.String("Int"))
                    ))
                )
            ))
        );
        var args = new MintContractArgs
        {
            ContractName = "parseInt",
            Pre = pre,
            ProducedBy = "csharp-kit@1.0",
            ProducedAt = "2026-04-30T00:00:00.000Z",
            Authoring = new Authoring.KitAuthor("csharp-kit@1.0"),
            SignerSeed = DummySeed(),
        };
        var minted = Mint.MintContract(args);
        Assert.StartsWith("blake3-512:", minted.Cid);
        // Full 64-byte digest = 128 hex chars, plus the 11-char prefix.
        Assert.Equal("blake3-512:".Length + 128, minted.Cid.Length);
        // Canonical bytes must contain the producerSignature field and the
        // self-identifying ed25519 prefix.
        var s = System.Text.Encoding.UTF8.GetString(minted.CanonicalBytes);
        Assert.Contains("\"producerSignature\":\"ed25519:", s);
        Assert.Contains("\"propertyHash\":\"blake3-512:", s);
        Assert.Contains("\"bindingHash\":\"blake3-512:", s);
        Assert.Contains("\"verdict\":\"holds\"", s);
    }

    [Fact]
    public void BridgeCid_IsBlake3_512_FullForm()
    {
        var args = new MintBridgeArgs
        {
            ProducedBy = "csharp-kit@1.0",
            ProducedAt = "2026-04-30T00:00:00.000Z",
            SourceSymbol = "parseInt",
            SourceLayer = "csharp-kit",
            TargetContractCid = "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c01",
            TargetLayer = "dotnet",
            IrArgSorts = new[] { "String" },
            IrReturnSort = "Int",
            SignerSeed = DummySeed(),
        };
        var minted = Mint.MintBridge(args);
        Assert.StartsWith("blake3-512:", minted.Cid);
        Assert.Equal("blake3-512:".Length + 128, minted.Cid.Length);
    }

    [Fact]
    public void ImplicationCid_IsBlake3_512_FullForm()
    {
        var args = new MintImplicationArgs
        {
            ProducedBy = "csharp-kit@1.0",
            ProducedAt = "2026-04-30T00:00:00.000Z",
            AntecedentHash = "blake3-512:" + new string('a', 128),
            ConsequentHash = "blake3-512:" + new string('b', 128),
            AntecedentCid = "blake3-512:" + new string('c', 128),
            ConsequentCid = "blake3-512:" + new string('d', 128),
            AntecedentSlot = "pre",
            ConsequentSlot = "post",
            Prover = "z3",
            ProverRunMs = 12,
            SignerSeed = DummySeed(),
        };
        var minted = Mint.MintImplication(args);
        Assert.StartsWith("blake3-512:", minted.Cid);
        Assert.Equal("blake3-512:".Length + 128, minted.Cid.Length);
    }

    [Fact]
    public void Mint_DeterministicAcrossRuns()
    {
        var pre = V.Object(
            ("kind", V.String("atomic")),
            ("name", V.String(">")),
            ("args", V.Array(
                V.Object(
                    ("kind", V.String("var")),
                    ("name", V.String("n"))
                ),
                V.Object(
                    ("kind", V.String("const")),
                    ("value", V.Integer(0)),
                    ("sort", V.Object(
                        ("kind", V.String("primitive")),
                        ("name", V.String("Int"))
                    ))
                )
            ))
        );
        MintContractArgs Build() => new()
        {
            ContractName = "parseInt",
            Pre = pre,
            ProducedBy = "csharp-kit@1.0",
            ProducedAt = "2026-04-30T00:00:00.000Z",
            Authoring = new Authoring.KitAuthor("csharp-kit@1.0"),
            SignerSeed = DummySeed(),
        };
        var a = Mint.MintContract(Build());
        var b = Mint.MintContract(Build());
        Assert.Equal(a.Cid, b.Cid);
        Assert.Equal(a.CanonicalBytes, b.CanonicalBytes);
    }
}
