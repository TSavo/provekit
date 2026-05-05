// SPDX-License-Identifier: Apache-2.0
//
// Backward WP propagation from callsite to allocation over Rust source.
// Implements the launch-demo algorithm from issue #368.
//
// Scope of this MVP:
//  - Parse Rust source via `syn`.
//  - Locate every callsite to a target callee in a caller function.
//  - Walk the caller's body backward through statements, applying
//    Dijkstra's WP rules at each step.
//  - Emit a chain of (WP, location) arrivals from callsite back to
//    function entry, plus the final WP at function entry as the
//    proof obligation that must be discharged by the caller's caller.
//  - Use `provekit-ir-types::IrFormula` as the canonical predicate
//    representation so downstream code can content-address arrivals
//    via the existing JCS pipeline.
//
// Out of scope for the MVP (tracked separately in #368 stretch goals):
//  - rustc MIR integration (this MVP uses surface AST; MIR is planned).
//  - Match-arm narrowing, while-let, if-let.
//  - Cross-function call-graph propagation.
//  - The dropper / generative-completion path.
//  - C kit (Clang CFG).
//  - Pointer aliasing.

pub mod canonical;
pub mod chain;
pub mod charon_runner;
pub mod contract;
pub mod emit;
pub mod envelope;
pub mod lift;
pub mod llbc;
pub mod llbc_lift;
pub mod marriage;
pub mod locus;
pub mod loops_and_exceptions;
pub mod shadow;
pub mod type_decl;
pub mod walk;
pub mod wp;

pub use canonical::{
    cid_of_value, formula_to_canonical, jcs_bytes_of_value, serde_to_canonical, term_to_canonical,
};
pub use envelope::{
    mint_args, wrap_function_contract, wrap_function_contract_cached, EnvelopeCache,
    DEV_SIGNER_SEED,
};
pub use lift::{lift_function_postcondition, lift_function_precondition, lift_predicate};
pub use shadow::{
    build_shadow_source, compose_chain, compose_edges, edge_memento_cid, edge_memento_value,
    CalleeContract, ComposedEdge, ShadowArrival, ShadowSlot, ShadowSource,
};
pub use walk::{walk_callsites_to_entry, Arrival, CallsiteWalk};
pub use wp::{
    atomic_ge, atomic_lt, atomic_true, const_int, free_vars_formula, free_vars_term, var, Wp,
};
