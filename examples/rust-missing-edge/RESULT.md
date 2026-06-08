# rust-missing-edge: the missing edge, CAUGHT through the real verbs

This crate demonstrates the founding claim end-to-end through the real CLI:
**a bug is a missing edge** -- a callsite whose precondition no producer's
postcondition establishes, caught by lift + compose + discharge, *not* by a
unit test.

The bug:

- `serialize(value) -> i64` returns `value`. The lifter derives the post
  `result == value`. It says **nothing** about the sign of the result.
- `content_address(encoding) -> i64` has a leading guard
  `if encoding < 0 { panic!(...) }`, which lifts to the precondition
  `pre = (encoding >= 0)`.
- `address_of(value) = content_address(serialize(value))` is the seam.
  `serialize`'s post (`result == value`) does **not** establish
  `content_address`'s pre (`encoding >= 0`): for `value < 0` the precondition
  is violated and the program panics.
- The unit test `address_of_nonneg_round_trips` calls `address_of(7)`, which
  satisfies the (undischarged) precondition by luck. **The test passes. The bug
  hides from the test.**

## What the real verbs do (run 2026-05-27, release build)

```
$ cargo test                 # address_of_nonneg_round_trips ... ok   (bug hides)

$ sugar mint --project examples/rust-missing-edge --out .../proofs --no-attest
mint ok                      # the rust lifter emits serialize/content_address/address_of contracts

$ sugar prove examples/rust-missing-edge
Sugar verifier report
  total callsites : 3
  discharged      : 2
  violations      : 1

  [unsatisfied] content_address  (source -> kit)
      reason: solver 'z3' returned sat (counterexample found)
  [discharged]  serialize        (source -> kit)   unsat (obligation holds)
  [discharged]  address_of       (source -> kit)   unsat (obligation holds)
```

**The missing edge is CAUGHT.** `prove` composes `address_of`'s body, finds the
`content_address(serialize(value))` callsite, and discharges the obligation
`serialize.post -> content_address.pre`. z3 returns **sat**: there is a
`value < 0` (the region the source panics on, and the test never exercises) for
which `serialize`'s postcondition fails to establish `content_address`'s
precondition. `serialize` and `address_of` discharge (unsat: they hold). The
bug the unit test misses, refuted by the solver.

## How it was caught: three stacked holes (all below the language line)

Before this work, `prove` reported the buggy crate as `discharged: 1,
violations: 0` -- a **false green**. Three holes had to be closed, each exposed
by the previous fix:

1. **Pool category error (verifier, `MementoPool::insert`).** A contract's
   `preHash` was indexed into `formula_to_memento`, the map Tier 0 (`verify`)
   trusts as "this formula is proven true". A precondition is an *obligation*,
   not a fact: a callsite's consumer-pre self-discharged merely because the
   callee *declared* it. Fix: index only `postHash`/`invHash`/`consequentHash`
   (established facts); `preHash`/`antecedentHash` are obligations.

2. **Vacuous lift (lifter, `lift_expr_to_term_inner`).** A plain function call
   `f(args)` had no lift arm (only method calls did), so `address_of`'s body
   lifted to `None` and its postcondition collapsed to a vacuous `true`. With
   no `content_address(...)` ctor in any formula, the seam was invisible to
   `enumerate_callsites`. Fix: lift `Expr::Call` to `Ctor{name, args}`, so the
   call tree survives into the contract. (Callsites: 1 -> 3.)

3. **Discharge expected a quantifier the lifter never emits (verifier).** Both
   solver paths (`instantiate`, `build_implication_obligation`) require the
   pre/post quantified as `forall`, but body-derived contracts emit *bare*
   predicates over named formals. Also `locate_producer_post` read the v1.1
   `evidence.body` shape while mint emits v1.2 `header`, so the producer post
   never resolved. Fixes: synthesize `forall (formal). pre` and
   `forall result. post` at the discharge boundary from the contract's own
   `formals`; make `locate_producer_post` shape-agnostic; normalize
   integer-width binder sorts (`I64`...) to the SMT `Int` the verifier already
   reasons in.

**What this exposed about the substrate's history:** holes (1) and (3) together
mean the *solver* discharge path had effectively never run on a real
body-derived lift before this session -- every real callsite discharged at
Tier 0 (hash lookup) or vacuously. Fixing the Tier 0 category error routed real
obligations into the solver for the first time, which is how holes (3a)/(3b)
surfaced at all.

**Why it generalizes:** holes (1) and (3) are in the verifier, operating on the
protocol's formulas and hashes -- not on any source language. Hole (2)'s
*principle* (a lifter emits the body call-tree as ctors referencing the
callee's bridge symbol) is universal; the Rust arm is local, the catch is not.
A missing edge in Go, Java, or Python reduces to the identical IR shape -- a
callsite whose consumer-pre no producer-post establishes. One verifier refutes
them all. The bug the unit tests miss is caught at the substrate, below the
language.

## Known follow-ups (not this change; tracked separately)

- **Multi-formal callsites.** The callsite model tracks a single arg term, so
  the forall-wrap binds the first formal only. Multi-arg discharge is a
  pre-existing limitation.
- **`emit_sort` integer widths.** The generated SMT emitter passes primitive
  sort names through verbatim; `I64` (and other widths) reach z3 as unknown
  sorts. The binder normalization here keeps obligations in LIA; the deeper fix
  is to teach `emit_sort` the integer-width families.
- **`targetProofCid` back-compat.** The auto-minted bridges carry no
  `targetProofCid`, so `ConsequentBundlePinned` is not enforced (a soft warning
  on every callsite).

Re-run the three verbs above to reproduce the catch.
