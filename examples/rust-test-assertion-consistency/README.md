# Rust Test-Assertion Consistency

This receipt is the Rust parity analog of `examples/python-consistency-dummy`.
It lifts plain `#[test]` scalar assertions into inv-only contracts and lets the
verifier's consistency pass check raw satisfiability.

- `good/`: `assert_same(make_value(), 6)` reduces through the visible helper to
  equality and is SAT, so the consistency row is discharged.
- `bad/`: `assert_same(make_value(), 6)` and `assert_same(make_value(), 7)`
  reduce through the same helper and are UNSAT together, so the consistency row
  is refused.
