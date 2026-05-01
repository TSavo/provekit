// SPDX-License-Identifier: Apache-2.0
//
// provekit-macros-rt
//
// Runtime support for the `#[provekit::contract(...)]` and
// `#[provekit::verify]` attribute macros (see sibling crate
// `provekit-macros`). Proc-macro crates can ONLY export proc-macros,
// so the registration types and the `inventory::collect!` invocations
// live here, where ordinary library code can reach them.
//
// The decorator and the `.invariant.rs` surfaces are two front-ends
// over the same primitives. Both ultimately produce
// `provekit_ir_symbolic::ContractDecl` values; the difference is only
// WHERE the author writes them and HOW the orchestrator collects them.
//
//   * `.invariant.rs` files are pulled in via `#[path]` by
//     `provekit-self-contracts/src/lib.rs`. The orchestrator calls
//     each `invariants()` and drains the kit's thread-local
//     `CONTRACT_COLLECTOR`.
//
//   * `#[provekit::contract]` annotations submit a
//     `ContractRegistration` into the `inventory` crate's distributed
//     slice. The orchestrator iterates `inventory::iter::<...>` and
//     calls each `builder` fn pointer.
//
// Both produce ContractDecls. The orchestrator merges per the
// conflict-resolution semantics in
// `protocol/specs/2026-04-30-contract-merge-semantics.md`.
//
// Why the indirection (a builder fn pointer instead of the
// ContractDecl directly)?  Because `inventory::submit!` requires the
// submitted value to be `Sync + 'static`, and `ContractDecl` carries
// `Rc<Formula>` (not Sync) and owned `String`s. The builder fn
// pointer IS Sync; calling it lazily constructs the ContractDecl in
// the orchestrator's thread.

use provekit_ir_symbolic::ContractDecl;

// ---------------------------------------------------------------------------
// Contract registrations (the #[provekit::contract] surface)
// ---------------------------------------------------------------------------

/// One static registration emitted by the `#[provekit::contract]`
/// macro. The macro generates a free function whose pointer goes in
/// `builder`; that function calls into the kit primitives to assemble
/// a ContractDecl on demand.
///
/// `name` defaults to the decorated function's identifier; the macro's
/// `name = "..."` argument can override it.
///
/// `source_path` and `source_line` come from `file!()` and `line!()`
/// at expansion time. They are not load-bearing for proof minting but
/// power the merge-semantics spec's "fail-loud at build time" error
/// reports (telling the author WHERE the conflicting authors live).
pub struct ContractRegistration {
    pub name: &'static str,
    pub source_path: &'static str,
    pub source_line: u32,
    pub builder: fn() -> ContractDecl,
}

inventory::collect!(ContractRegistration);

/// Iterate every `#[provekit::contract]` registration in the linked
/// binary. Stable ordering: insertion order within a translation unit
/// is preserved by `inventory`, but cross-TU order is implementation
/// defined. The orchestrator must sort by `name` before minting if
/// determinism across builds is required.
pub fn registered_contracts() -> impl Iterator<Item = &'static ContractRegistration> {
    inventory::iter::<ContractRegistration>()
}

/// Convenience: drive every registered builder and collect the
/// resulting ContractDecls. This is what the self-contracts
/// orchestrator calls on the macro side. It pairs with the
/// `.invariant.rs` walk on the file side.
pub fn collect_all_contracts() -> Vec<ContractDecl> {
    let mut out: Vec<ContractDecl> = registered_contracts().map(|r| (r.builder)()).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

// ---------------------------------------------------------------------------
// Verify-target registrations (the #[provekit::verify] surface)
// ---------------------------------------------------------------------------

/// One static registration emitted by `#[provekit::verify]` on a
/// function definition. Signals to the build script (TODO: not yet
/// wired; the builder lives in a follow-up task) that the function's
/// body should be scanned for call sites, each of which is then
/// dispatched against the corresponding contract.
///
/// `ast_hash_hint` is filled by the macro with a coarse digest of the
/// function's input tokens at expansion time. It is NOT a reproducible
/// AST hash (the protocol's canonical AST hash lives in the lifter,
/// not here); the macro uses it only as a quick "did this function
/// body change?" signal to invalidate cached call-site enumerations.
pub struct VerifyTarget {
    pub fn_name: &'static str,
    pub source_path: &'static str,
    pub source_line: u32,
    pub ast_hash_hint: u64,
}

inventory::collect!(VerifyTarget);

pub fn registered_verify_targets() -> impl Iterator<Item = &'static VerifyTarget> {
    inventory::iter::<VerifyTarget>()
}

// ---------------------------------------------------------------------------
// Re-exports for the macros' generated code.
//
// The macros expand to `::provekit_macros_rt::__priv::...`; keeping
// the public re-export surface narrow lets us evolve the macro
// expansion without breaking downstream consumer code.
// ---------------------------------------------------------------------------

#[doc(hidden)]
pub mod __priv {
    pub use inventory;
    pub use provekit_ir_symbolic;
    pub use super::{ContractRegistration, VerifyTarget};
}
