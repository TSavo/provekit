// SPDX-License-Identifier: Apache-2.0
//
// Fixture crate for the end-to-end smoke test.
//
// Three deliberate annotation strategies are mixed here so that the smoke
// test's "zero contracts authored by us" receipt has something to prove:
//
//   1. Functions in src/option_handling.rs carry `#[requires]` / `#[ensures]`
//      attributes. The smoke-test driver LIFTS those to formulas; the
//      driver itself authors nothing.
//
//   2. Functions in src/validate_then_commit.rs carry NO contract
//      annotations, but the test in `tests/properties.rs` asserts a
//      property over them. The smoke-test driver LIFTS the assertion
//      to a witness formula; the driver itself authors nothing.
//
//   3. Functions in src/retry_with_backoff.rs carry NEITHER annotations
//      NOR test assertions. The smoke-test driver SYNTHESIZES a
//      contract by applying the wp_rule for the recognized structural
//      shape (loop-with-bounded-attempts). The driver applies the rule;
//      the contract is dictated by the term algebra, not by us.
//
// The fixture is small but multi-shape on purpose: it surfaces real
// compression (two retry-with-backoff variants) and a real propagation
// event (one assertion attaching to a concept and inheriting across
// every binding to the same concept-CID).

pub mod clamps;
pub mod option_handling;
pub mod retry_with_backoff;
pub mod validate_then_commit;
