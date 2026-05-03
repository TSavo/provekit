// SPDX-License-Identifier: Apache-2.0
//
// CallEdgeDeclaration — call-edge memento emitted by lifters per
// protocol/specs/2026-05-03-bridge-linkage-protocol.md §1 R1.
//
// Wire form (spec §1):
//
//   {
//     "schemaVersion":     "1",
//     "kind":              "call-edge",
//     "sourceContractCid": <CID of the calling function's contract>,
//     "targetContractCid": <CID of the called function's contract> | null,
//     "callSiteLocus":     { "file", "line", "column" },
//     "targetSymbol":      <kit-prefixed symbol for linker resolution>,
//     "evidenceTerm":      <ProofIR term placeholder>
//   }
//
// When the callee is within the same kit, both sourceContractCid and
// targetContractCid are populated and targetSymbol is the local name.
// For cross-kit calls (e.g. P/Invoke, cgo), targetContractCid is null
// and targetSymbol carries the kit-prefixed form "rust-kit:Process"
// that the linker resolves per R3.
// If the library is unresolvable, targetSymbol is "resolver-error:<name>"
// and the linker promotes it to a linker-error memento.

namespace Provekit.IR;

/// <summary>File+line+column call-site position per ir-formal-grammar.md.</summary>
public sealed record Locus(string File, int Line, int Column);

/// <summary>
/// A call-edge memento per bridge-linkage-protocol.md §1.
/// Emitted by the lifter for every call site within a lifted compilation unit.
/// </summary>
public sealed record CallEdgeDeclaration(
    string SourceContractCid,
    string? TargetContractCid,
    string TargetSymbol,
    Locus CallSiteLocus,
    string EvidenceTerm);
