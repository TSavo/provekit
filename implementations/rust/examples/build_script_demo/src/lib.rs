// SPDX-License-Identifier: Apache-2.0
//
// ProvekIt build-script integration demo.
//
// This crate's `build.rs` invokes `provekit_build::run_verification()`,
// which source-walks `src/` for `#[provekit::contract(...)]` and
// `#[provekit::verify]` annotations and runs a Tier-3 Z3 check on
// each verify body's call sites. The expected output of
// `cargo build --release` includes a `--- ProvekIt verification
// report ---` block on stderr plus zero or more
// `cargo:warning=provekit: ...` lines.

use provekit_ir_symbolic::{forall, gte, num, out, Int};
use provekit_macros::{contract, verify};

// `abs_value` returns a non-negative i64 for any input. The
// post-condition is `out >= 0`. `use_abs` calls it; nothing in the
// body conflicts with the contract, so the verifier discharges the
// call site cleanly.
#[contract(post = forall(Int(), |_| gte(out(), num(0))))]
pub fn abs_value(x: i64) -> i64 {
    x.abs()
}

#[verify]
pub fn use_abs(n: i64) {
    let x = abs_value(n);
    // `x` is always >= 0 per the contract; this addition is fine.
    let _ = x + 1;
}

// `always_positive` returns at least 1 by construction. The
// post-condition is `out >= 1`.
#[contract(post = forall(Int(), |_| gte(out(), num(1))))]
pub fn always_positive() -> i64 {
    5
}

// The `if x == 0` branch is dead per the contract: `x >= 1` and
// `x == 0` cannot hold simultaneously. The verifier picks this up
// from the surrounding equality check on the call site's binding
// (see `provekit_build::source_walk::CallSite::surrounding_eq_check`)
// and Z3 returns `unsat` on the conjunction.
//
// In `strict = true` mode this would fail the build. The demo runs
// with `strict = false` so the result surfaces as a warning only.
#[verify]
pub fn deliberate_violation() {
    let x = always_positive();
    if x == 0 {
        panic!("can't reach here per the contract");
    }
}

