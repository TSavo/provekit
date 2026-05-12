// SPDX-License-Identifier: Apache-2.0
//
// Test-lift source.
//
// The smoke-test driver scans this file for `assert!` / `assert_eq!`
// invocations under `#[test]` fns. For each assertion whose LHS is the
// return value of one of the fixture functions, the driver builds an
// `IrFormula` post-condition and binds it to the concept-CID of that
// function's call site.
//
// Only `commit_balance_change` has an assertion here. The substrate
// then propagates the witness to every other binding of the same
// concept-CID (in this fixture, `commit_inventory_change`).

use smoke_test_e2e::validate_then_commit::commit_balance_change;

#[test]
fn commit_balance_change_post_is_non_negative() {
    // post: out >= 0
    let result = commit_balance_change(10, -5);
    assert!(result >= 0);
}
