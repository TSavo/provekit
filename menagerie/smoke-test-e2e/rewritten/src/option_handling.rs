// rewritten by smoke-test-e2e-driver pass 1
//
// Every contract attribute and concept annotation below was emitted
// by the substrate. None were written by the driver author. See
// report.md §8 for the per-line origin trace.

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

// concept: UNNAMED-CONCEPT-2
// substrate-origin: annotation-lift
// memento-cid: blake3-512:e84f8b18667407182caae176db0798259d12763f941c580611bcbf489d43b9f0df864049f8b8e19a866a4eaa0f8fb78a17a49d5ca0424593ff63b4ea6b91c976
#[cfg_attr(any(), requires(items_len >= 0))]
#[cfg_attr(any(), ensures(out == if items_len == 0 { 0 } else { 1 }))]
pub fn first_or_default(items_len: i64) -> i64 {
    if items_len == 0 {
        0
    } else {
        1
    }
}

// concept: UNNAMED-CONCEPT-3
// substrate-origin: annotation-lift
// memento-cid: blake3-512:e3a27191078d666aa6f158919e2b9e8c6402dc93f997587f6d387f2329fb698d1dcb6eb3fb285f8725f7b715b7e5f4ae5026e843790635ef81d9acd5c53fd8f9
#[cfg_attr(any(), requires(idx >= 0))]
#[cfg_attr(any(), ensures(out >= 0))]
pub fn safe_index(idx: i64, len: i64) -> i64 {
    if idx < len {
        idx
    } else {
        len.saturating_sub(1).max(0)
    }
}
