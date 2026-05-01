# build-script-demo

End-to-end demo of `provekit-build`. Shows the four-line opt-in shape,
two contract lanes (inventory + lift), and a deliberately-violating
call site that surfaces in the verifier's report.

## TL;DR

Just `cargo build`. Lift + verify run automatically. No separate
commands. There is no `cargo provekit-lift`; the adapters fire from the
build script.

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
warning: build-script-demo@0.1.0: provekit: lift promoted 3 contract(s) from third-party annotations [proptest=2, contracts=1]
warning: build-script-demo@0.1.0: provekit: WARN .../src/lib.rs::deliberate_violation -> always_positive: violation: branch `if x == 0` is dead per contract (post=gte_const)
    Finished `release` profile [optimized] target(s) in 3.47s
```

The verification report on the build-script's stderr (visible at
`target/release/build/build-script-demo-*/stderr`):

```
--- ProvekIt verification report ---
  inventory contracts: 2
  lift contracts:      3
  total contracts:     5
    lift breakdown:    proptest(2/2), contracts(1/1), kani(0/0), prusti(0/0), creusot(0/0), flux(0/0), quickcheck(0/0), verus(0/0)
  verify targets:      2
  callsites:           2
    discharged:        1
    unsatisfied:       1
    undecidable:       0
  proof file:          .../build-script-demo-<hash>/out/provekit/<cid>.proof
  proof cid:           blake3-512:7e8e6de2...
  strict mode:         false
  z3 path:             z3
  z3 timeout (ms):     3000
  summary:             2 inventory, 3 lift, 1 verified, 1 violation(s)
------------------------------------
```

## Two lanes

The demo carries contracts on two lanes that the build script keeps
parallel:

### Inventory lane (`src/lib.rs`)

The kit's own decorators. `cargo build` source-walks `src/`, finds
every `#[provekit::contract]` and `#[provekit::verify]` annotation, and
runs the Tier-3 Z3 check on each verify body's call sites.

  * `abs_value` carries `post = forall(Int(), |_| gte(out(), num(0)))`.
  * `always_positive` carries `post = forall(Int(), |_| gte(out(), num(1)))`.
  * `use_abs` is a `#[verify]` whose call site discharges cleanly.
  * `deliberate_violation` is a `#[verify]` whose `if x == 0` branch is
    dead per the contract — Z3 returns `unsat` and the build script
    emits a `cargo:warning=`. With `strict = true` this would fail the
    build.

### Lift lane (`src/lift_examples.rs`)

Third-party annotations the consumer already had. `cargo build`
dispatches each parsed source file to all eight registered lift
adapters (proptest, contracts, kani, prusti, creusot, flux,
quickcheck, verus). The lifters return rich `ContractDecl` IR
which gets minted into the same `.proof` manifest as the inventory
lane.

  * `proptest! { fn identity_holds(x: i64) { prop_assert_eq!(x, x); } }`
    plus `fn nonneg_after_abs(x: i64) { prop_assert!(x >= 0); }` —
    lifted by the proptest adapter into two universally-quantified
    invariants.
  * `#[contracts::requires(x > 0)] #[contracts::ensures(ret >= 0)] fn lifted_sqrt` —
    lifted by the contracts adapter into a ContractDecl with both pre
    and post slots populated.

#### Why a separate file

`src/lift_examples.rs` is not declared via `mod lift_examples;` in
`lib.rs`, so rustc never compiles it; the source-walker and lift
adapters scan every `.rs` file under `src/` regardless of `mod`
declarations. That lets the demo show the proptest / contracts
annotations without dragging the proptest / contracts crates into the
demo's actual compile graph. A real consumer who already uses those
crates simply leaves their annotations where they are — the lift pass
finds them.

## What the demo proves

1. `cargo build --release` discovers contracts on TWO lanes:
     * 2 inventory contracts via `#[provekit::contract]` decorators.
     * 3 lift contracts via proptest / contracts annotations.
2. Both lanes feed the proof manifest; the resulting `<cid>.proof` is
   content-addressed over the union.
3. The verifier still runs Tier-3 Z3 on the inventory lane's call
   sites; lift-derived contracts are reported and minted but don't
   yet drive the SMT round-trip (deliberate v0 boundary).
4. The `use_abs -> abs_value` call site is **discharged**.
5. The `deliberate_violation -> always_positive` call site is
   **unsatisfied** (`unsat` on the conjunction). With `strict = true`
   this would fail the build.
6. A signed `<cid>.proof` is minted under `OUT_DIR`.

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

That's it. No `cargo provekit-lift` step.

## Adapter whitelist

To narrow the lift pass to a subset of adapters, list them in the
metadata table:

```toml
[package.metadata.provekit]
lift_adapters = ["proptest", "contracts"]
```

Omit the field (or set it to `None` from a programmatic config) to run
every registered adapter (the default). An empty list disables the
lift pass entirely.

## Try strict mode

Edit `Cargo.toml`:

```toml
[package.metadata.provekit]
strict = true
```

Re-run `cargo build`. The build now exits non-zero with the same
warning escalated to an error, producing a `cargo:warning=provekit:
ERROR ...` line and a non-zero status from the build script.
