# Polars Showcase

This mirrors `examples/numpy-showcase` for Rust: one real Polars scalar test is
lifted on two axes.

- `rust-test-assertions` lifts scalar `#[test]` assertions into closed
  consistency obligations. The good suite is SAT and discharges; the bad suite
  asserts the same scalar equals both `6` and `7`, so it is UNSAT and refused.
- `rust-cargo-test-witness` reruns `cargo test` and discharges only when the
  witnessed test package passes.

The fixture intentionally stays on plain scalar Rust assertions. A future Polars
increment can add Polars-specific assertion vocabulary if it needs richer
DataFrame/Series equality semantics.
