// rewritten by smoke-test-e2e-driver pass 1
//
// Every contract attribute and concept annotation below was emitted
// by the substrate. None were written by the driver author. See
// report.md §8 for the per-line origin trace.

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

// concept: guard-then-commit
// substrate-origin: test-lift
// memento-cid: blake3-512:4d94e0456ff4bb908e9b77a7b9bb45eb4debb5ca2814224c82acbbcdd88268dfee3ce515a700110e72142c180dc4c307679d91735c12c9be1ac21b93ced872ec
#[cfg_attr(any(), ensures(out >= 0))]
#[cfg_attr(any(), witness(out >= 0))]
// witness-inherited-from: properties.rs:assert
pub fn commit_balance_change(current: i64, delta: i64) -> i64 {
    let proposed = current + delta;
    if proposed >= 0 {
        proposed
    } else {
        current
    }
}

/// Validates non-negativity and updates inventory level.

// concept: guard-then-commit
// substrate-origin: algebra-synthesis[wp_rule.guard-then-commit.v0]
// memento-cid: blake3-512:9e270257e1258a3a82e1e39b84fd4f036cbe590ae60616845ec714e0b9b5e500ce6612bddbb19fb8795907adf3f9f065a58479777b503a9f834f0903efe787da
#[cfg_attr(any(), ensures((out >= 0) || (out == before_state)))]
#[cfg_attr(any(), witness(out >= 0))]
// witness-inherited-from: properties.rs:assert
pub fn commit_inventory_change(level: i64, delta: i64) -> i64 {
    let proposed = level + delta;
    if proposed >= 0 {
        proposed
    } else {
        level
    }
}
