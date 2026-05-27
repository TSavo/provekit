# rust-missing-edge: a NEGATIVE result through the real verbs

This crate was built to demonstrate the founding claim, end-to-end through the
real CLI: **a bug is a missing edge** -- a callsite whose precondition no
producer's postcondition establishes, caught by lift + compose + discharge,
*not* by a unit test.

The bug:

- `serialize(value) -> i64` returns `value`. The lifter derives the post
  `result == value`. It says **nothing** about the sign of the result.
- `content_address(encoding) -> i64` has a leading guard
  `if encoding < 0 { panic!(...) }`, which lifts to the precondition
  `pre = NOT(encoding < 0)`.
- `address_of(value) = content_address(serialize(value))` is the seam.
  `serialize`'s post (`result == value`) does **not** establish
  `content_address`'s pre (`encoding >= 0`): at `value = -1` the precondition
  is violated and the program panics.
- The unit test `address_of_nonneg_round_trips` calls `address_of(7)`, which
  satisfies the (undischarged) precondition by luck. **The test passes. The bug
  hides.**

## What the real verbs actually do (run 2026-05-27, release build)

```
$ provekit mint --project examples/rust-missing-edge --out .../proofs --no-attest --json
mint ok: True            # produced a .proof

$ provekit verify examples/rust-missing-edge
verify: lifting contract claims from `examples/rust-missing-edge`
verify: no contract claims found for `examples/rust-missing-edge`; nothing to verify
  (lift the kit first: `provekit mint --kit=...` or run the kit's lifter)

$ provekit prove examples/rust-missing-edge
warning: bridge address_of has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
ProvekIt verifier report
  total callsites : 1
  discharged      : 1
  violations      : 0
  load errors     : 0

  [discharged] address_of  (source -> kit)
      reason: tier0: memento-is-verification (cid=blake3-512:unknown...)
```

`cargo test` also passes (`address_of_nonneg_round_trips ... ok`).

**The missing edge is NOT caught.** The buggy crate reports
`discharged: 1, violations: 0`. This is a **false green**.

## Why (root cause, traced through the verifier)

1. `provekit verify` (the kit-claim gate) enumerates `@sugar`/`@boundary`
   contract *claims* only. This crate has none, so verify finds nothing.

2. `provekit prove` enumerates callsites in
   `provekit-verifier/src/enumerate_callsites.rs`: it walks each *contract*
   memento's `pre`/`post`/`inv` formulas for ctor terms whose `name` matches a
   **declared bridge** `sourceSymbol` (`pool.bridges_by_symbol`). **An
   intra-body call statement (`content_address(serialize(value))`) is not a
   bridge**, so the `serialize -> content_address` seam is never enumerated as
   an obligation. The only callsite found is `address_of` (a bridge the lifter
   emitted, source -> kit).

3. That `address_of` callsite discharged at **Tier 0**
   (`provekit-verifier/src/runner.rs:870` -- "Memento IS verification"):
   `pool.verify(consumer_pre)` matched because, for a body-derived contract, the
   consumer's `pre` is trivially already in the pool (it was minted from the
   same body). Discharge by hash lookup, **not by solving**. The solver
   (Tier 3) is never reached.

4. The bridge carried no `targetProofCid`, so `resolve_target.rs:54` took the
   **back-compat path** and did not enforce `ConsequentBundlePinned` (hence the
   warning).

5. The rust lifter *does* have intra-body callsite composition
   (`provekit-walk/src/llbc_calls.rs::compose_callsite_pre`, wired in
   `llbc_lift.rs:1205`), but (a) it runs only on the LLBC/Charon lift path, not
   the `provekit-walk-rpc` path this crate's `.provekit/config.toml` uses, and
   (b) it is **weakest-precondition propagation** (`out.push` of the callee's
   pre into the *caller's* pre contributions), **not** a refusable obligation.
   Even through LLBC, `address_of` would simply acquire the precondition
   `value >= 0`; the test at `value = 7` still satisfies it, still no refusal.

## The question this leaves for the substrate (T's call)

Tier 0 discharges a body-derived callsite because the consumer's `pre` is
always present in the pool (minted from the same body). If that is sound, then
**no body-derived callsite ever reaches the solver** -- "missing edge = caught
bug" is realizable only through *declared boundaries*, not through plain
intra-body composition, which is exactly what this demo was built to exercise.

Whether the Tier-0 back-compat discharge on unannotated rust is intended
behavior, or a hole that lets a real missing edge pass as a false green, is a
substrate-scope decision, not a phrasing one.

Re-run the three verbs above to reproduce the false green.
