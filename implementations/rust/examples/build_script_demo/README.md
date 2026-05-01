# build-script-demo

End-to-end demo of `provekit-build`. Shows the four-line opt-in shape
and a deliberately-violating call site that surfaces in the verifier's
report.

## Run it

```
cd implementations/rust/examples/build_script_demo
cargo clean -p build-script-demo
cargo build --release
```

Expected output (excerpted):

```
   Compiling provekit-build v0.1.0 (.../provekit-build)
   Compiling build-script-demo v0.1.0 (.../examples/build_script_demo)
warning: build-script-demo@0.1.0: provekit: WARN .../src/lib.rs::deliberate_violation -> always_positive: violation: branch `if x == 0` is dead per contract (post=gte_const)
    Finished `release` profile [optimized] target(s) in 2.03s
```

The verification report on the build-script's stderr (visible at
`target/release/build/build-script-demo-*/stderr`):

```
--- ProvekIt verification report ---
  contracts:        2
  verify targets:   2
  callsites:        2
    discharged:     1
    unsatisfied:    1
    undecidable:    0
  proof file:       .../build-script-demo-<hash>/out/provekit/<cid>.proof
  proof cid:        blake3-512:7e8e6de2...
  strict mode:      false
  z3 path:          z3
  z3 timeout (ms):  3000
------------------------------------
```

## What the demo proves

1. `cargo build --release` discovers two `#[contract]` annotations
   (`abs_value`, `always_positive`) and two `#[verify]` annotations
   (`use_abs`, `deliberate_violation`).
2. The verifier emits an SMT-LIB script per call site and dispatches
   to Z3 with a 3-second timeout.
3. The `use_abs -> abs_value` call site is **discharged**: the
   contract `out >= 0` is realizable; nothing in the body conflicts.
4. The `deliberate_violation -> always_positive` call site is
   **unsatisfied**: the contract is `out >= 1`, the body's `if x ==
   0` branch is dead, Z3 returns `unsat` on the conjunction. With
   `strict = true` this would fail the build.
5. A signed `<cid>.proof` is minted under `OUT_DIR`.

## The four-line opt-in

`Cargo.toml`:

```toml
[build-dependencies]
provekit-build = "0.1"

[package.metadata.provekit]
strict = false
```

`build.rs`:

```rust
fn main() { provekit_build::run_verification(); }
```

## Try strict mode

Edit `Cargo.toml`:

```toml
[package.metadata.provekit]
strict = true
```

Re-run `cargo build`. The build now exits non-zero with the same
warning escalated to an error, producing a `cargo:warning=provekit:
ERROR ...` line and a non-zero status from the build script.
