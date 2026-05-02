// Fixture: a representative slice of liftable #[kani::*] annotations.
// NOT compiled; the lift adapter parses it as text via syn.
//
// Liftable: 4 requires/ensures pairs over simple comparisons.
// Skipped: should_panic marker, plus a fancy method-call expression.

#[kani::requires(x > 0)]
#[kani::ensures(result >= 0)]
fn kani_sqrt(x: i64) -> i64 { x }

#[kani::requires(n >= 0)]
#[kani::ensures(result > 0)]
fn factorial(n: i64) -> i64 { 1 }

#[kani::requires(a < b)]
#[kani::ensures(result == a)]
fn min(a: i64, b: i64) -> i64 { a }

#[kani::requires(divisor != 0)]
#[kani::ensures(result <= dividend)]
fn safe_div(dividend: i64, divisor: i64) -> i64 { 0 }

// Skip path: should_panic markers describe negation, not predicate
// shape; v0 logs and skips.
#[kani::proof]
#[kani::should_panic]
fn always_panics() {
    let _ = 1 / 0;
}

// Skip path: method call inside the predicate is outside the v0
// whitelist (var/lit/single-arg-call only). Expected to surface as a
// warning.
#[kani::requires(s.len() > 0)]
fn nonempty(s: String) -> i64 { 0 }
