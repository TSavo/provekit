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

fn attempt_succeeds(attempt: i64) -> bool {
    // Stand-in for an oracle: succeeds on the second attempt onward.
    attempt >= 2
}
