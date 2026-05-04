// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance test for Provekit.IR.BridgeDeclaration
// (the v1.1.0 spec-shaped 9-field record).
//
// Fixture: `bridge_decl_v1_1` from conformance/fixtures.toml.
// See protocol/specs/2026-04-30-ir-formal-grammar.md (BridgeDeclaration).
//
// Build the BridgeDeclaration with all 9 fields, JCS-encode through the
// canonicalizer Value tree, and assert byte-equality against the fixture
// JCS literal. JCS sorts keys at emit time, so insertion order in the
// emitter is irrelevant; the test pins the wire form.
//
// This is the spec record, NOT the lift-helper BridgeDecl in Collector.cs
// (which carries TargetContractName / IrArgSorts / IrReturnSort, a
// different shape used by the lift adapter). They coexist by design.

using Provekit.Canonicalizer;
using Provekit.IR;
using Xunit;

namespace Provekit.Tests;

public class BridgeDeclarationConformanceTests
{
    // From conformance/fixtures.toml [[fixture]] name = "bridge_decl_v1_1".
    // (Renamed from `bridge_decl` in PR-1 of issue #219 v1.4 migration.)
    // JCS-sorted keys: kind, name, notes, sourceContractCid, sourceLayer,
    // sourceSymbol, targetContractCid, targetLayer, targetProofCid.
    private const string ExpectedFixtureJcs =
        @"{""kind"":""bridge""," +
        @"""name"":""myBridge""," +
        @"""notes"":""some notes""," +
        @"""sourceContractCid"":""bafySource""," +
        @"""sourceLayer"":""c-kit""," +
        @"""sourceSymbol"":""source""," +
        @"""targetContractCid"":""bafyTarget""," +
        @"""targetLayer"":""coq""," +
        @"""targetProofCid"":""bafyProof""}";

    [Fact]
    public void BridgeDeclaration_AllFields_MatchesFixtureJcs()
    {
        var b = new BridgeDeclaration(
            Name: "myBridge",
            SourceSymbol: "source",
            SourceLayer: "c-kit",
            SourceContractCid: "bafySource",
            TargetContractCid: "bafyTarget",
            TargetProofCid: "bafyProof",
            TargetLayer: "coq",
            Notes: "some notes");

        var v = Serialize.BridgeDeclarationToValue(b);
        var bytes = Jcs.Encode(v);

        Assert.Equal(ExpectedFixtureJcs, bytes);
    }

    [Fact]
    public void BridgeDeclaration_OmittedNotes_DoesNotEmitNotesKey()
    {
        // notes is optional: when absent, the JCS output must not include
        // a "notes" key at all (no `"notes":null`, no empty string). The
        // spec marks it `<string?, optional>`; serde on the Rust peer uses
        // `skip_serializing_if = "Option::is_none"`. We mirror that.
        var b = new BridgeDeclaration(
            Name: "myBridge",
            SourceSymbol: "source",
            SourceLayer: "c-kit",
            SourceContractCid: "bafySource",
            TargetContractCid: "bafyTarget",
            TargetProofCid: "bafyProof",
            TargetLayer: "coq",
            Notes: null);

        var v = Serialize.BridgeDeclarationToValue(b);
        var bytes = Jcs.Encode(v);

        Assert.DoesNotContain("notes", bytes);
        Assert.DoesNotContain("null", bytes);
    }
}
