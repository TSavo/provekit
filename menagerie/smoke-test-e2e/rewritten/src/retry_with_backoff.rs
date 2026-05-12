// rewritten by smoke-test-e2e-driver pass 1
//
// Every contract attribute and concept annotation below was emitted
// by the substrate. None were written by the driver author. See
// report.md §8 for the per-line origin trace.

// SPDX-License-Identifier: Apache-2.0
//
// Algebra-synthesis example.
//
// Neither function below has any contract annotation. Neither has a
// test assertion. Both nonetheless have a recognizable structural shape
// the term algebra knows about: a bounded-attempt retry loop. The
// smoke-test driver classifies the lifted term shape against the
// concept-shape catalog. When the shape matches `concept:retry-with-bounded-attempts`,
// the driver applies the wp_rule registered for that concept and
// synthesizes a contract automatically.
//
// Crucially: `try_send_v1` and `try_send_v2` are written with
// SLIGHTLY DIFFERENT control flow (one uses a `while`, one uses a
// `for`; one increments before retry, one increments after; one
// returns on success inside the loop, one returns after the loop).
// They still cluster to the SAME concept-CID because the canonical
// term shape is the same. That is the compression event.

/// Variant 1: `while` with pre-increment and early return inside.

// concept: retry-with-bounded-attempts
// substrate-origin: algebra-synthesis[wp_rule.retry-with-bounded-attempts.v0]
// memento-cid: blake3-512:88c92b7c05ab0997b6d7faf585cbb03a39dbe8caca8baa4ac21f050104fe88ff688491896b9efebde78c3ad4b59f40da5077c592e21d90be72b8279d9088726f
#[cfg_attr(any(), requires(max_attempts >= 0))]
#[cfg_attr(any(), ensures((out == true) || (out == false)))]
pub fn try_send_v1(max_attempts: i64) -> bool {
    let mut attempt = 0;
    while attempt < max_attempts {
        attempt += 1;
        if attempt_succeeds(attempt) {
            return true;
        }
    }
    false
}

/// Variant 2: `for` with the success check after the call.

// concept: retry-with-bounded-attempts
// substrate-origin: algebra-synthesis[wp_rule.retry-with-bounded-attempts.v0]
// memento-cid: blake3-512:bc6ceef4548ebbd9628f0baaabc6fa1fc0b6ca6b494f9d6861f159971e8da1f5d7a70df219a1252d2edeedfa079b5153fab4909b10fc73e5e34982b6d20f2b7c
#[cfg_attr(any(), requires(max_attempts >= 0))]
#[cfg_attr(any(), ensures((out == true) || (out == false)))]
pub fn try_send_v2(max_attempts: i64) -> bool {
    let mut succeeded = false;
    for attempt in 1..=max_attempts {
        if attempt_succeeds(attempt) {
            succeeded = true;
            break;
        }
    }
    succeeded
}

// concept: UNNAMED-CONCEPT-4
// substrate-origin: empty
fn attempt_succeeds(attempt: i64) -> bool {
    // Stand-in for an oracle: succeeds on the second attempt onward.
    attempt >= 2
}
