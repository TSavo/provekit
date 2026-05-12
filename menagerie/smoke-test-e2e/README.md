# smoke-test-e2e: end-to-end vision demo

A small Rust fixture under `src/` and a driver under `driver/` that
together exercise the eight-verb pipeline from paper 20:

    Lift -> Cluster -> Name -> Scope -> Cluster -> Identify -> Realize -> Witness.

Run it from the workspace root:

```
cargo run -p smoke-test-e2e-driver
```

The driver lifts the fixture, clusters by canonical term-shape, names
each cluster (catalog or `UNNAMED-CONCEPT-N`), scopes bindings,
identifies them through wp_rule / annotation / test sources, propagates
witnesses, compresses near-shape variants, and emits a fully
substrate-attributed `rewritten/` tree plus the load-bearing
`report.md`.

Then it does it again. The second pass reads the human-supplied
`// concept: <name>` annotation written into the rewritten output and
learns the new name. The `report.md` is the artifact to read first.

## Outputs after one run

```
artifacts/                            signed mementos + stub site mementos
rewritten/Cargo.toml                  rewritten crate
rewritten/src/*.rs                    substrate-attributed source
rewritten/tests/                      copied tests
report.md                             the substrate-voice report
```

## Layout

```
src/                  fixture (parsed by syn, NOT compiled before lift)
  clamps.rs           unnamed cluster -> renamed by the round-trip demo
  option_handling.rs  annotation-lift demo
  retry_with_backoff.rs  algebra-synthesis + compression demo
  validate_then_commit.rs  test-lift + propagation demo
tests/properties.rs   the one unit test driving the witness propagation
driver/               the driver crate (eight-verb pipeline + report)
```

## Zero-authoring receipt

Every contract that lands in `rewritten/` comes from one of three
substrate sources:

1. ANNOTATION-LIFT: a `#[requires]` / `#[ensures]` attribute in the
   fixture source (kept inert under `rustc` via `cfg_attr(any(), ...)`).
2. TEST-LIFT: an `assert!` in `tests/properties.rs` whose LHS resolves
   through a `let ... = <fn>(...)` binding to a fixture function.
3. ALGEBRA-SYNTHESIS: a wp_rule registered for a recognized term-shape
   classification (`retry-loop`, `guard-then-commit`).

The driver author wrote no contracts. The report's section 8 traces
every emitted contract back to its substrate origin.
