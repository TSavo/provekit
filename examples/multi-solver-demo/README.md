# Multi-Solver Demo

Drives the verifier four times against the same fixture obligation,
once per mode defined in
[`protocol/specs/2026-04-30-multi-solver-protocol.md`](../../protocol/specs/2026-04-30-multi-solver-protocol.md):

* **single (default)**: invoke one solver, return its verdict
* **chain**: sequential fall-through; first definitive verdict wins
* **portfolio first-wins**: parallel via rayon; first definitive verdict wins
* **portfolio consensus**: parallel; ALL must agree, otherwise loud disagreement
* **per-fragment dispatch**: pick the matching solver by formula theory

Uses `binary = "stub:..."` solvers so the demo runs in CI without any
solver binaries installed. Swap to a real binary path
(`binary = "z3"`, `binary = "cvc5"`, ...) in your project's
`.sugar/config.toml` to drive real solvers.

## Run

```sh
cargo run --release -p sugar-verifier --example multi_solver_demo
```

## Sample output (abbreviated)

```
=== mode: single (default) ===
  verdict: discharged
  reason : solver 'z3' returned unsat (obligation holds)
  per-solver invocations:
    - solver=z3             version=stub-unsat verdict=discharged    wall_ms=   0 timed_out=false

=== mode: chain ===
  verdict: discharged
  reason : chain: solver 'cvc5' (step 2/2) returned discharged ...
  per-solver invocations:
    - solver=z3             version=stub-undecidable verdict=undecidable   wall_ms=   0 timed_out=false
    - solver=cvc5           version=stub-unsat       verdict=discharged    wall_ms=   0 timed_out=false

=== mode: portfolio first-wins ===
  verdict: discharged
  reason : portfolio[first-wins]: 'z3' returned discharged in 0ms

=== mode: portfolio consensus (agree) ===
  verdict: discharged
  reason : portfolio[consensus]: 2 solvers agree on discharged

=== mode: portfolio consensus (DISAGREEMENT) ===
  verdict: disagreement
  reason : portfolio[consensus]: SOLVER DISAGREEMENT: z3=discharged, cvc5=unsatisfied

=== mode: per-fragment dispatch ===
  verdict: discharged
  reason : solver 'z3' returned unsat (obligation holds)
```
