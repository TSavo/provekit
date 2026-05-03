// SPDX-License-Identifier: Apache-2.0
//
// Phase-2 cross-kit bridge tests: csharp-kit attestation that the C#
// lift adapter satisfies the Rust kit's `lift_plugin_protocol`
// contracts. Mirrors PR #89 (python), PR #92 (go), PR #93 (ts).
//
// Pinned: BLAKE3-512 of the JCS-canonical bytes of the
// BridgeDeclaration array returned by
// CrossKitBridges.BuildBridges() with TargetContractCid carrying the
// `pending-csharp-counterpart:<name>` placeholder. The placeholder
// shape IS what's frozen here so the bridge list itself is content-
// addressable independent of the transient C# bundle's internal CIDs.
//
// Drift in any of the following invalidates this hash:
//   - rust contract memento CID for any of the 10 lift-plugin-protocol
//     contracts (CrossKitBridges.RustContractCids)
//   - bridge name spelling (bridge_to_<rust_name>)
//   - C# counterpart name spelling (csharp_<rust_name>)
//   - sourceLayer / targetLayer / notes literals
//   - DeferredCsharpLiftBinaryCid ("deferred:phase-3-proof-bundle")
//   - declaration order
//   - JCS emitter, BLAKE3-512 hasher, BridgeDeclaration JCS shape

using Provekit.Canonicalizer;
using Provekit.IR;
using Provekit.SelfContracts.Invariants;
using Xunit;

namespace Provekit.Tests;

public class CrossKitBridgesTests
{
    /// <summary>
    /// Frozen BLAKE3-512 of the JCS-canonical bytes of the 10 phase-2
    /// cross-kit bridges. See module-level comment for drift surfaces.
    /// </summary>
    private const string PinnedBridgesArrayCid =
        "blake3-512:160f9d47eed498769805494f4ae1854ac82fd009cc0467cbc726faec0257b42449369a2bd9d1037672049031597252b72b5d3c8a4e47ceb69d3e2dba77714c55";

    [Fact]
    public void RustContractCids_HasExactlyTenEntries()
    {
        Assert.Equal(10, CrossKitBridges.RustContractCids.Count);
    }

    [Fact]
    public void RustContractCids_AreWellFormedBlake3_512()
    {
        const string wantPrefix = "blake3-512:";
        foreach (var pair in CrossKitBridges.RustContractCids)
        {
            Assert.StartsWith(wantPrefix, pair.Value);
            var hex = pair.Value.Substring(wantPrefix.Length);
            Assert.Equal(128, hex.Length);
            foreach (var c in hex)
            {
                Assert.True(
                    (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'),
                    $"non-lowercase-hex char in CID for {pair.Key}: {c}");
            }
        }
    }

    [Fact]
    public void BuildBridges_ReturnsTenWithCorrectShape()
    {
        var bridges = CrossKitBridges.BuildBridges();
        Assert.Equal(10, bridges.Count);

        foreach (var b in bridges)
        {
            Assert.StartsWith("bridge_to_lift_plugin_", b.Name);
            Assert.Equal(CrossKitBridges.RustKitLayer, b.SourceLayer);
            Assert.Equal(CrossKitBridges.CsharpKitLayer, b.TargetLayer);
            Assert.Equal(CrossKitBridges.DeferredCsharpLiftBinaryCid, b.TargetProofCid);
            Assert.Equal(CrossKitBridges.Phase2BridgeNotes, b.Notes);
            Assert.StartsWith("blake3-512:", b.SourceContractCid);
            Assert.StartsWith(
                CrossKitBridges.PendingTargetContractCidPrefix,
                b.TargetContractCid);
        }

        // Stable order: bridges must appear in rust-declaration order.
        var expectedNames = CrossKitBridges.RustContractCids
            .Select(p => CrossKitBridges.BridgeName(p.Key))
            .ToArray();
        var actualNames = bridges.Select(b => b.Name).ToArray();
        Assert.Equal(expectedNames, actualNames);
    }

    [Fact]
    public void BuildBridges_EachBridgePointsAtCorrectRustSourceCid()
    {
        var bridges = CrossKitBridges.BuildBridges();
        var rustMap = CrossKitBridges.RustContractCids
            .ToDictionary(p => p.Key, p => p.Value);
        foreach (var b in bridges)
        {
            Assert.True(rustMap.TryGetValue(b.SourceSymbol, out var expectedCid),
                $"bridge {b.Name}: SourceSymbol {b.SourceSymbol} not in rust map");
            Assert.Equal(expectedCid, b.SourceContractCid);

            var expectedTarget = CrossKitBridges.PendingTargetContractCid(
                CrossKitBridges.CounterpartContractName(b.SourceSymbol));
            Assert.Equal(expectedTarget, b.TargetContractCid);
        }
    }

    [Fact]
    public void BridgesArrayCid_MatchesPinnedHash()
    {
        // Pinned BLAKE3-512 of JCS(BuildBridges() encoded as a JSON array
        // of BridgeDeclaration objects). If this fails, dump the actual
        // value, verify the inputs are intentional, and update the pin.
        var actual = CrossKitBridges.ComputeBridgesArrayCid();
        if (actual != PinnedBridgesArrayCid)
        {
            // Surface the full actual hash so xUnit's truncated diff
            // doesn't hide it. Re-throw via Assert.Fail with the full
            // value embedded in the message.
            Assert.Fail(
                $"phase-2 bridges JCS hash drift:\n  pinned: {PinnedBridgesArrayCid}\n  actual: {actual}\n\n" +
                "If this drift is intentional, update PinnedBridgesArrayCid " +
                "and re-sign the C# self-contracts attestation in a follow-up.");
        }
    }
}
