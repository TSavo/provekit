// SPDX-License-Identifier: Apache-2.0
//
// v1.1.0 spec-shaped BridgeDeclaration. Mirrors the
// `Declaration::Bridge` variant in
// implementations/rust/provekit-ir-types/src/lib.rs and the
// `bridge_decl` fixture in conformance/fixtures.toml.
//
// Per protocol/specs/2026-04-30-ir-formal-grammar.md (BridgeDeclaration),
// the wire form is a 9-field object:
//
//   {
//     "kind":              "bridge",
//     "name":              <string>,
//     "sourceSymbol":      <string>,
//     "sourceLayer":       <string>,
//     "sourceContractCid": <CID>,
//     "targetContractCid": <CID>,
//     "targetProofCid":    <CID>,
//     "targetLayer":       <string>,
//     "notes":             <string?, optional>
//   }
//
// `notes` is omitted from the JCS output when null (matches the Rust
// peer's `skip_serializing_if = "Option::is_none"`).
//
// This record is DISTINCT from `Provekit.IR.BridgeDecl` in Collector.cs,
// which is a lift-adapter helper carrying TargetContractName /
// IrArgSorts / IrReturnSort. They serve different purposes and coexist
// by design; do not unify them.

namespace Provekit.IR;

public sealed record BridgeDeclaration(
    string Name,
    string SourceSymbol,
    string SourceLayer,
    string SourceContractCid,
    string TargetContractCid,
    string TargetProofCid,
    string TargetLayer,
    string? Notes = null);
