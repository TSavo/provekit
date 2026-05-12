// SPDX-License-Identifier: Apache-2.0
//
// Test-lift example.
//
// Neither `commit_balance_change` nor `commit_inventory_change` has a
// contract annotation in the source. The test in
// `tests/properties.rs` asserts a post-condition over
// `commit_balance_change` only. The smoke-test driver lifts that
// assertion to an `IrFormula` and attaches it to the concept-CID for
// the validate-then-commit shape.
//
// Both functions cluster to the same concept-CID (their lifted term
// shape is the same: guard then mutate). The propagation event
// inherits the test-derived witness onto `commit_inventory_change`
// even though that function has no test of its own.

/// Validates non-negativity and credits the new balance.
pub fn commit_balance_change(current: i64, delta: i64) -> i64 {
    let proposed = current + delta;
    if proposed >= 0 {
        proposed
    } else {
        current
    }
}

/// Validates non-negativity and updates inventory level.
pub fn commit_inventory_change(level: i64, delta: i64) -> i64 {
    let proposed = level + delta;
    if proposed >= 0 {
        proposed
    } else {
        level
    }
}
