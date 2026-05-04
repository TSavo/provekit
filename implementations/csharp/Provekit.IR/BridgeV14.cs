// SPDX-License-Identifier: Apache-2.0
//
// v1.4 BridgeDeclaration IR (layered envelope/header/body, tagged-union target).
//
// Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1-R6
// and 2026-05-03-substrate-layers-envelope-header-body.md §1.
//
// Canonical reference: implementations/rust/provekit-claim-envelope/src/lib.rs
//   fn mint_bridge_v14 (line 595), BridgeTargetV14 (line 486).
//
// Differences from v1.1 BridgeDeclaration (Bridge.cs):
//   1. target is a tagged-union object {kind, cid}, not flat targetContractCid.
//   2. Header carries the contract-axis claim only (spec §1.R3).
//   3. Metadata fields unknown at mint time are OMITTED (no null, no placeholders).
//   4. Layered shape: {envelope, header, metadata}.

using System.Text.Json.Serialization;

namespace Provekit.IR;

// ── Tagged-union target ─────────────────────────

/// <summary>
/// Discriminated union per spec §1.R1. Exactly one variant.
/// </summary>
public abstract record BridgeTargetV14
{
    public abstract string Kind { get; }
    public abstract string Cid { get; }

    public sealed record Contract(string Cid) : BridgeTargetV14
    {
        public override string Kind => "contract";
    }

    public sealed record ContractSet(string Cid) : BridgeTargetV14
    {
        public override string Kind => "contractSet";
    }
}

// ── Header (7 fields, substrate-verified) ───────

/// <summary>
/// Per spec §1.R3: carries the contract-axis claim only.
/// </summary>
public sealed record BridgeHeaderV14(
    string Name,
    string SourceSymbol,
    string SourceLayer,
    string SourceContractCid,
    BridgeTargetV14 Target)
{
    public string SchemaVersion => "1"; // v1.4 layered schema
    public string Kind => "bridge";
}

// ── Metadata (6 optional axes, None => omitted) ──

/// <summary>
/// Per spec §1.R2: only Some fields are emitted into JCS bytes.
/// None fields are OMITTED (no null, no placeholder strings).
/// </summary>
public sealed record BridgeMetadataV14(
    string? TargetWitnessCid = null,
    string? TargetBinaryCid = null,
    string? TargetLayer = null,
    string? TargetContractSetCid = null,
    string? ProducedBy = null,
    string? ProducedAt = null);

// ── Mint inputs ─────────────────────────────────

/// <summary>
/// Inputs for <see cref="ClaimEnvelope.MintBridgeV14"/>.
/// </summary>
public sealed class MintBridgeV14Args
{
    // -- header fields --
    public required string Name { get; init; }
    public required string SourceSymbol { get; init; }
    public required string SourceLayer { get; init; }
    public required string SourceContractCid { get; init; }
    public required BridgeTargetV14 Target { get; init; }

    // -- metadata fields (optional) --
    public string? TargetWitnessCid { get; init; }
    public string? TargetBinaryCid { get; init; }
    public string? TargetLayer { get; init; }
    public string? TargetContractSetCid { get; init; }
    public string? ProducedBy { get; init; }
    public string? ProducedAt { get; init; }

    // -- envelope fields --
    public required string DeclaredAt { get; init; }
    public required byte[] SignerSeed { get; init; }
}
