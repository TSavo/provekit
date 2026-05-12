// SPDX-License-Identifier: Apache-2.0
//
// Trinity round-trip fixture.
//
// Uses the 11 trinity catalog concepts:
//   option, pair, result, tagged-union, identity, list,
//   unit, assert, bool-cell, option-bind, result-bind
//
// Also includes a retry-loop shape so that seed_catalog() produces
// at least one classified binding on leg 1, giving the test a
// non-empty translated/java/ tree to chain from.

// ── identity ────────────────────────────────────────────────────────────────
// concept: identity
// substrate-origin: annotation-lift
pub fn wrap_identity(x: i64) -> i64 {
    x
}

// ── unit ────────────────────────────────────────────────────────────────────
// concept: unit
pub fn do_nothing() {}

// ── bool-cell ───────────────────────────────────────────────────────────────
// concept: bool-cell
pub fn toggle(flag: bool) -> bool {
    !flag
}

// ── assert ───────────────────────────────────────────────────────────────────
// concept: assert
pub fn assert_positive(x: i64) -> i64 {
    if x <= 0 {
        panic!("assert_positive: x must be > 0, got {x}");
    }
    x
}

// ── option ───────────────────────────────────────────────────────────────────
// concept: option
pub fn maybe_first(items: &[i64]) -> i64 {
    if items.is_empty() {
        -1
    } else {
        items[0]
    }
}

// ── option-bind ──────────────────────────────────────────────────────────────
// concept: option-bind
pub fn option_bind_double(items: &[i64]) -> i64 {
    if items.is_empty() {
        -1
    } else {
        let v = items[0];
        if v <= 0 {
            -1
        } else {
            v * 2
        }
    }
}

// ── result ───────────────────────────────────────────────────────────────────
// concept: result
pub fn safe_divide(num: i64, denom: i64) -> i64 {
    if denom == 0 {
        -1
    } else {
        num / denom
    }
}

// ── result-bind ──────────────────────────────────────────────────────────────
// concept: result-bind
pub fn safe_divide_then_double(num: i64, denom: i64) -> i64 {
    if denom == 0 {
        -1
    } else {
        let q = num / denom;
        if q < 0 {
            -1
        } else {
            q * 2
        }
    }
}

// ── pair ─────────────────────────────────────────────────────────────────────
// concept: pair
pub fn swap_pair(a: i64, b: i64) -> (i64, i64) {
    (b, a)
}

// ── list ─────────────────────────────────────────────────────────────────────
// concept: list
pub fn list_sum(items: &[i64]) -> i64 {
    let mut acc = 0i64;
    for &v in items {
        acc += v;
    }
    acc
}

// ── tagged-union ─────────────────────────────────────────────────────────────
// concept: tagged-union
pub fn classify(x: i64) -> i64 {
    if x < 0 {
        0
    } else if x == 0 {
        1
    } else {
        2
    }
}

// ── retry-loop (seed_catalog shape: ensures classified binding in leg 1) ─────
// concept: retry-loop
#[cfg_attr(any(), requires(max_attempts > 0))]
#[cfg_attr(any(), ensures(out == true))]
pub fn retry_until_success(max_attempts: i64) -> bool {
    let mut attempt = 0;
    while attempt < max_attempts {
        attempt += 1;
        if attempt >= 1 {
            return true;
        }
    }
    false
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trinity_smoke() {
        assert_eq!(wrap_identity(7), 7);
        do_nothing();
        assert_eq!(toggle(true), false);
        assert_eq!(assert_positive(3), 3);
        assert_eq!(maybe_first(&[10, 20]), 10);
        assert_eq!(maybe_first(&[]), -1);
        assert_eq!(option_bind_double(&[4]), 8);
        assert_eq!(option_bind_double(&[]), -1);
        assert_eq!(safe_divide(10, 2), 5);
        assert_eq!(safe_divide(10, 0), -1);
        assert_eq!(safe_divide_then_double(6, 3), 4);
        assert_eq!(swap_pair(1, 2), (2, 1));
        assert_eq!(list_sum(&[1, 2, 3]), 6);
        assert_eq!(classify(-5), 0);
        assert_eq!(classify(0), 1);
        assert_eq!(classify(3), 2);
        assert!(retry_until_success(3));
    }
}
