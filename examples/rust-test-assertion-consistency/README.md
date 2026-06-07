# Rust Test-Assertion Consistency

This receipt is the Rust parity analog of `examples/python-consistency-dummy`.
It lifts plain `#[test]` scalar assertions into inv-only contracts and lets the
verifier's consistency pass check raw satisfiability.

- `good/`: `assert_eq!(make_value(), 6)` is SAT, so the consistency row is
  discharged.
- `bad/`: `assert_eq!(make_value(), 6)` and `assert_eq!(make_value(), 7)` are
  UNSAT together, so the consistency row is refused.
