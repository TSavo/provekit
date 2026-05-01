// SPDX-License-Identifier: Apache-2.0
//
// Demonstrates the `#[provekit::contract(...)]` and `#[provekit::verify]`
// authoring surfaces. Same kit primitives as the `.invariant.rs`
// surface, decorator placement.
//
// Run with:
//
//   cargo run --release --example macro_smoke
//
// What this example proves operationally:
//
//   1. The decorator on each function emits a hidden static
//      ContractRegistration registered via `inventory`.
//   2. `provekit_macros_rt::collect_all_contracts()` walks the
//      registrations and returns ContractDecls (the same type the
//      `.invariant.rs` surface drops into the kit's
//      thread-local CONTRACT_COLLECTOR).
//   3. The orchestrator side (provekit-self-contracts) can mint each
//      ContractDecl into a signed contract memento; conflict-resolution
//      across the two surfaces follows
//      protocol/specs/2026-04-30-contract-merge-semantics.md.
//
// What this example does NOT prove:
//
//   * Build-script wiring for `#[provekit::verify]` (the call-site
//     enumeration walker) is a follow-up task. Here we only show the
//     marker registers in the inventory slice.

use provekit_ir_symbolic::{eq, forall, gt, lt, num, Int, String_};
use provekit_macros::{contract, verify};
use provekit_macros_rt::{collect_all_contracts, registered_contracts, registered_verify_targets};

// A function with only a precondition. Default name = function ident.
#[contract(pre = forall(Int(), |n| gt(n, num(0))))]
fn parse_positive_int(s: &str) -> i64 {
    s.parse::<i64>().unwrap_or(1).max(1)
}

// A function with both pre and post. `out` references the return value
// in the post slot. `out_binding` defaults to "out".
#[contract(
    pre = forall(String_(), |s| eq(s.clone(), s)),
    post = forall(Int(), |n| gt(n, num(-1)))
)]
fn measure_string(_s: &str) -> i64 {
    42
}

// A function with a custom name and an invariant. `name` overrides the
// fn ident in the resulting ContractDecl (but the registration's
// `name` field still carries the ident for debugging).
#[contract(
    name = "loop_body_correct",
    inv = forall(Int(), |n| lt(n, num(1000))),
    out_binding = "y"
)]
fn loop_body() -> i64 {
    0
}

// A function with the verify marker. The build script (TODO follow-up)
// will scan its body for call sites and dispatch each against the
// matching contract.
#[verify]
fn verified_caller(s: &str) -> i64 {
    parse_positive_int(s) + measure_string(s) + loop_body()
}

fn main() {
    println!("=== ProvekIt macro smoke test ===\n");

    println!("Registered contracts (via inventory::iter):");
    for r in registered_contracts() {
        println!(
            "  {:<20} @ {}:{}",
            r.name, r.source_path, r.source_line
        );
    }

    println!("\nMaterialized ContractDecls (sorted, ready to mint):");
    for d in collect_all_contracts() {
        println!(
            "  {:<28} pre={} post={} inv={} out_binding={}",
            d.name,
            d.pre.is_some(),
            d.post.is_some(),
            d.inv.is_some(),
            d.out_binding
        );
    }

    println!("\nRegistered verify targets:");
    for t in registered_verify_targets() {
        println!(
            "  {:<20} @ {}:{}  hash_hint=0x{:016x}",
            t.fn_name, t.source_path, t.source_line, t.ast_hash_hint
        );
    }

    // Touch the decorated functions so the optimizer can't elide them.
    let _ = verified_caller("3");

    println!("\nOK.");
}
