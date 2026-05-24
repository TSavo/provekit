# Go end-to-end example: a Go library gets a contract, and the spine verifies it

This is the Go analog of the Rust production-bridge gauntlet
(`implementations/rust/provekit-cli/tests/cmd_verify_production_bridge.rs`). It
demonstrates that ProvekIt's verification spine is LANGUAGE-NEUTRAL: a Go
function's body-derived contract lifts to ProofIR
(`post = result == <value-expr>` plus `formals`) and the verifier discharges
the obligation through the body via weakest-precondition + z3, exactly as it
does for Rust and Java.

## The library

```go
func Double(x int) int { return x * 2 }
```

`double_test.go` carries the harvested assertion `assert.Equal(t, Double(3), 6)`.

## The chain (no hand-bridging, no hand-written contracts)

1. The **real Go lifter** (`provekit-lift-go-verify`, the verify-facing `go`
   lift surface) lifts:
   - `double.go` -> `function-contract` with `formals: ["x"]` and
     `post = result == (* x 2)` (the verify-facing dialect normalizes Go's
     `go:mul` to the SMT-LIB core symbol `*` so z3 can reduce it),
   - `double_test.go` -> `contract` with `inv = =(Double(3), 6)` (the Go
     Layer-0 leaf assertion harvester).
2. `provekit mint` AUTO-WRITES the bridge `Double -> targetContractCid` for the
   body-bearing function-contract (#1443). The bridge is TOOL-written, not
   hand-built.
3. `provekit verify`:
   - **positive**: reduces `Double(3) == 6` through the body to
     `(* 3 2) == 6` -> z3 discharges -> signed witness, exit 0.
   - **negative** (break the body to `x * 3`): the obligation becomes
     `(* 3 3) == 6` -> z3 refutes -> Unsatisfied, exit 1, no witness.

## Run it

```sh
cd implementations/go
go build -o /tmp/provekit-lift-go-verify ./cmd/provekit-lift-go-verify
# point this example's .provekit/lift/go/manifest.toml command[0] at the binary
# (or put it on PATH), then from the repo root:
provekit mint   --project examples/go-double --out examples/go-double --no-attest
provekit verify --project examples/go-double --emit-witnesses /tmp/go-witnesses
```

The gating integration test
(`implementations/rust/provekit-cli/tests/cmd_verify_go_production_bridge.rs`)
builds the binary, copies this example into a tempdir, and asserts both the
positive discharge (signed witness, exit 0) and the honest negative
(Unsatisfied, exit 1, no witness).
