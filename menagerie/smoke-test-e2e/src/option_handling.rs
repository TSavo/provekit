// SPDX-License-Identifier: Apache-2.0
//
// Annotation-lift example.
//
// These functions carry `#[requires]` / `#[ensures]` attributes the
// smoke-test driver reads as substrate input. The annotations parse as
// attribute literals; the driver converts them into IR formula nodes
// via `provekit-ir-symbolic` and mints `contract` mementos via
// `provekit-claim-envelope`. We never type a contract into the
// rewritten output ourselves; every contract in `rewritten/` comes
// back out the other end of the lift.
//
// The `cfg_attr(any(), ...)` gate keeps the attributes parseable by
// `syn` (so the lifter sees `#[requires(...)]` verbatim) while making
// them inert under `cargo build` / `cargo test` (the `any()` predicate
// is always false). This is the same trick the broader `provekit-lift-contracts`
// crate's tests use to keep example annotations live as substrate while
// passing rustc.

#[cfg_attr(any(), requires(items_len >= 0))]
#[cfg_attr(any(), ensures(out == if items_len == 0 { 0 } else { 1 }))]
pub fn first_or_default(items_len: i64) -> i64 {
    if items_len == 0 {
        0
    } else {
        1
    }
}

#[cfg_attr(any(), requires(idx >= 0))]
#[cfg_attr(any(), ensures(out >= 0))]
pub fn safe_index(idx: i64, len: i64) -> i64 {
    if idx < len {
        idx
    } else {
        len.saturating_sub(1).max(0)
    }
}
