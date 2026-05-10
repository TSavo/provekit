// SPDX-License-Identifier: Apache-2.0
//
// LIFT FIXTURE: NOT IN THE COMPILED MODULE TREE.
//
// This file is intentionally NOT declared via `mod lift_examples;`
// anywhere. The build script's source-walker (and the lift pass)
// scans every `.rs` file under `src/` regardless of `mod` declarations,
// so the contents below are visible to the lift adapters but invisible
// to rustc. That means we can demonstrate `proptest!` and
// `#[contracts::requires]` annotations without needing the proptest /
// contracts crates as actual compile-time dependencies of the demo.
//
// What lives here:
//
//   1. A `proptest!` block. The `provekit-lift-proptest` adapter walks
//      its body looking for `prop_assert*` invocations and lifts each
//      `#[test] fn` into a ContractDecl.
//
//   2. A `#[contracts::requires(...)]` + `#[contracts::ensures(...)]`
//      annotated function. The `provekit-lift-contracts` adapter
//      classifies the attributes and lifts the predicates.
//
// Running `cargo build --release` from this crate's directory should
// surface BOTH of these in the build's `cargo:warning=` output
// alongside the inventory-lane contracts on `abs_value` and
// `always_positive` from `lib.rs`.

// ---------------------------------------------------------------------------
// proptest fixture: a property over the identity that x equals itself.
// The proptest adapter lifts `prop_assert_eq!` invocations into a
// ContractDecl whose `inv` is a forall-quantified equality.
// ---------------------------------------------------------------------------
proptest! {
    #[test]
    fn identity_holds(x: i64) {
        prop_assert_eq!(x, x);
    }

    #[test]
    fn nonneg_after_abs(x: i64) {
        prop_assert!(x >= 0);
    }
}

// ---------------------------------------------------------------------------
// contracts fixture: a function with both a precondition and a
// postcondition. The contracts adapter lifts each predicate into the
// matching slot of a ContractDecl.
// ---------------------------------------------------------------------------
#[contracts::requires(x > 0)]
#[contracts::ensures(ret >= 0)]
fn lifted_sqrt(x: i64) -> i64 {
    x
}
