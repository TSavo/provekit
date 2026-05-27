# BZ-DETERMINISM-001 — Canonical Byte Order (unestablished precondition)

## The bug

A consumer (`ContentAddress`) requires its input to be in **canonical** form. A
producer (`Serialize`) emits an encoding that is *valid* but **not guaranteed
canonical**. A unit test exercises `ContentAddress(Serialize(…))` and passes —
it never hits a non-canonical input. The bug is invisible at runtime.

Only **lifting to ProofIR** exposes it: the producer's postcondition does not
establish the consumer's precondition, so the composition obligation
`post(serialize) ⟹ canonical(out)` does **not** discharge. That is the seam.

This is the admission criterion: *a contract was written like a unit test, the
test passed, and only the lift + solver saw the bug.* No mock — the contracts
are lifted from idiomatic Go by the real lifter (guard→precondition lifting,
PR #1561; composite-literal lifting, PR #1562) and the obligation is discharged
by the real solver (z3 returns `unsatisfied` for the exhibit, `satisfied` for
the fixed state).

## States

- **lab** — the passing unit test (`go test`), bug invisible.
- **exhibit** — `Serialize` emits a non-canonical encoding; the lifted
  composition obligation is `unsatisfied` (the missing edge).
- **fixed** — `Serialize` returns a canonical encoding; the obligation
  `satisfied`.

## Honest disclosure of the model

The *literal* map-serialization byte-order bug (serialize a map by iteration
order; content-address requires sorted bytes) is what this species is named
for, but the canonical predicate here is modeled **arithmetically** — a
canonical encoding is non-negative — so that the obligation reduces to a form
the SMT solver can actually **refute** (z3 returns `unsatisfied` with a
counterexample). This is the same bug *class* (a consumer precondition the
producer does not establish, invisible to a passing test), demonstrated on real
lifted contracts and the real solver.

The literal byte-order instance needs two further lifter features so the
obligation is solver-evaluable rather than `undecidable`:
1. **predicate inlining** — inline a called boolean predicate
   (`CanonicalByteOrder(b)`) into the contract so it is interpreted, not an
   opaque `go:call(...)`; and
2. **slice/byte SMT-modeling** — model `[]byte{…}` literals and indexing so the
   solver can reason about byte order.

Both are tracked follow-ups. The fixed state likewise canonicalizes to a
constant (the lifter does not yet model conditional canonicalization for the
postcondition); that too is gated on the same lifter work.
