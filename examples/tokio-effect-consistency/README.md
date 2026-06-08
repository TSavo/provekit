# Tokio Effect Consistency Showcase

This is the first Rust async effects slice. It proves that the Rust assertion
consistency axis survives a real Tokio `.await` boundary, and checks the same
program with the cargo-test witness axis.

- `good/`: a `#[tokio::test]` assertion over `async_value().await == 6`
  discharges, and the Tokio test passes.
- `bad/`: contradictory assertions over the same awaited expression refuse as
  UNSAT, and the Tokio test fails.

The `.await` term is lifted structurally from Rust syntax. It is not keyed on
the name `tokio`.
