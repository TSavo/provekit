# ProvekIt compared to Kani, Prusti, Creusot (Rust-specific provers)

The Rust ecosystem has multiple verification tools, each targeting Rust's annotations, semantics, and ownership model. ProvekIt is not in the same category. ProvekIt is a protocol for content-addressing the verifications these tools produce.

This doc walks through how ProvekIt complements each, and when you'd use which.

## Quick comparison

| Tool | Category | What it does |
|---|---|---|
| **Kani** | Bounded model checker | Verifies Rust against `#[kani::proof]` annotations using CBMC. |
| **Prusti** | Verifier | Verifies Rust against `#[prusti::ensures]` and friends using Viper. |
| **Creusot** | Deductive verifier | Verifies Rust against `#[ensures]` using Why3. |
| **Flux** | Refinement-types verifier | Adds refinement types to Rust; verifies them at compile time. |
| **MIRAI** | Static analyzer with formal core | Walks Rust MIR and checks invariants. |
| **ProvekIt** | Protocol for distributing the above's output | Content-addressed substrate; not a verifier. |

If you're choosing between "use ProvekIt" and "use Kani," you've miscategorized. Use Kani to verify your Rust code; use ProvekIt to publish, distribute, and federate Kani's output.

## What each Rust prover does

### Kani

[`kani.rs`](https://github.com/model-checking/kani). Bounded model checking for Rust:

- Annotations: `#[kani::proof]`, `kani::any()`, `kani::assume()`, `kani::assert()`.
- Backend: CBMC (C Bounded Model Checker).
- Verifies properties like memory safety, arithmetic overflow, panic absence, user-specified assertions.
- Bound: depth-bounded; exhaustive within the bound.
- TCB: CBMC + Kani's MIR translator.

Strong fit for: panic absence, memory safety, integer overflow, undefined behavior detection, simple invariants.

### Prusti

[`prusti.org`](https://www.pm.inf.ethz.ch/research/prusti.html). Deductive verification for Rust:

- Annotations: `#[requires]`, `#[ensures]`, `#[invariant]`, `predicate!`, ghost code.
- Backend: Viper (an intermediate verification language) → Z3.
- Verifies functional correctness specifications.
- TCB: Viper + Z3 + Prusti's encoder.

Strong fit for: functional correctness contracts, complex invariants, separation logic for ownership.

### Creusot

[`creusot-rs`](https://github.com/creusot-rs/creusot). Deductive verification with Why3:

- Annotations: `#[ensures]`, `#[requires]`, `#[invariant]`, term-language `pearlite!`.
- Backend: Why3 → multiple provers (Z3, CVC5, Alt-Ergo, Eprover, etc.).
- Constructive proofs available; can produce Coq scripts.
- TCB: Why3 + chosen backend(s).

Strong fit for: rich logical specifications, constructive-proof export, multi-prover concurrence within a single tool.

### Flux

Refinement types for Rust:

- Annotations: type signatures with refinements, e.g., `fn divide(x: i32, y: i32{y != 0}) -> i32`.
- Backend: Liquid types over Z3.
- Compile-time verification.
- TCB: Z3 + Flux's encoder.

Strong fit for: lightweight refinements (numeric ranges, indexing safety), low-overhead annotation.

### MIRAI

Static analyzer with formal core:

- Annotations: ad-hoc, configurable.
- Backend: walks Rust MIR; uses Z3 for arithmetic.
- Strong fit for whole-program analysis with weaker assurance than Kani/Prusti/Creusot.

## What ProvekIt does

ProvekIt does not analyze Rust code, does not compile Rust, does not invoke a Rust-specific backend. It provides:

- **Lift adapters** (`provekit-lift-proptest`, `provekit-lift-contracts`, planned `provekit-lift-kani`, `provekit-lift-prusti`) that walk existing Rust annotations and emit canonical IR.
- **Verification via Z3 by default**, with the option to substitute other backends.
- **Content-addressed mementos** for each contract.
- **Cross-language bridges** (Rust contract → reference contract ← TypeScript contract).

The picture: Kani/Prusti/Creusot/Flux/MIRAI verify Rust code. ProvekIt's lift adapters can take their annotations as input. The output canonical IR is portable across the dependency graph.

## When you'd use Kani alone (no ProvekIt)

- Your codebase is pure-Rust.
- You're verifying memory safety / panic absence / overflow.
- You don't have polyglot consumers.
- You don't need to publish your verification results.

Kani is fast, well-integrated with `cargo`, and has a clean assurance story. For pure-Rust panic-absence verification, ProvekIt would add overhead with limited additional value.

## When you'd use Prusti alone

- Your codebase is pure-Rust.
- You need rich functional contracts.
- You're a single team writing Rust.
- You don't need cross-language proof transfer.

Prusti is more powerful than Kani; the assurance is comparable; the cost is more annotation effort. For pure-Rust functional-correctness work, Prusti alone suffices.

## When you'd use Creusot alone

- Your codebase is pure-Rust.
- You need or want constructive proofs.
- You're willing to commit to Why3-style annotations.
- You may want to export to Coq.

Creusot's constructive-proof export is a unique strength; if you need to satisfy regulators or auditors that require constructive proofs, Creusot alone may suffice.

## When you'd combine ProvekIt with these tools

ProvekIt adds value when one or more of:

- **You have Rust dependencies and non-Rust consumers** (or vice versa). The lift adapter promotes Rust annotations to canonical IR; bridges connect to non-Rust consumers' contracts.
- **You're publishing a Rust library to a registry** (`crates.io`). Shipping a `.proof` alongside the crate makes consumers' verification cheaper. Cache effects help.
- **You want supply-chain `binaryCid` pinning.** Kani/Prusti/Creusot don't pin compiled artifacts; ProvekIt's `binaryCid` adds this.
- **You want federated proof reuse.** A `(post, pre)` pair discharged by Kani in one project becomes available to every other project's verifier. Kani would re-run otherwise.

The combination is: use Kani/Prusti/Creusot to verify, use ProvekIt to publish and distribute.

## A worked example

Consider a Rust crate that's verified with Kani:

```rust
#[kani::proof]
fn parse_int_no_panic() {
    let s: String = kani::any();
    if s.len() <= 18 {
        let _ = s.parse::<i32>(); // verified to not panic for short strings
    }
}
```

Kani verifies the assertion. The result is local to your codebase.

With ProvekIt:

1. `provekit-lift-kani` (planned for v1.2) walks `#[kani::proof]` and lifts to canonical IR.
2. The lift produces a contract memento: "for `parse::<i32>()` on strings of length ≤ 18, no panic occurs."
3. You publish the contract memento + Kani's discharge as an implication memento.
4. A consumer (in any language with a `parse` equivalent) can bridge to this contract and inherit the verification.

Without ProvekIt, the consumer must either re-run Kani on their own test harness or trust the maintainer's claim. With ProvekIt, the discharge is content-addressed evidence.

## TCB comparison

For pure-Rust, single-team verification:

| Setup | TCB |
|---|---|
| Kani alone | CBMC (~50kloc C) + Kani encoder |
| Prusti alone | Viper + Z3 + Prusti encoder |
| Creusot alone | Why3 + chosen backend + Creusot encoder |
| ProvekIt + Kani lift | Above + protocol primitives + kit |

ProvekIt adds protocol-layer TCB. The trade is portability and federation.

For multi-language deployments, Kani/Prusti/Creusot can't help directly with non-Rust consumers; ProvekIt's bridge mechanism is the path.

## What's coming

Planned for v1.2 (per [`../../reference/per-language-status.md`](../../reference/per-language-status.md)):

- `provekit-lift-kani`: walks `#[kani::proof]` functions, `kani::assume`, `kani::assert`. Emits canonical IR for Kani-verified properties.
- `provekit-lift-prusti`: walks `#[prusti_contracts::requires]` / `#[prusti_contracts::ensures]`. Same shape.

Under evaluation:

- `provekit-lift-creusot`: rich logical fragments. Some annotations may need new IR primitives.
- `provekit-lift-flux`: refinement types. Partial lift expected.

When these adapters ship, the workflow becomes "verify with your Rust prover of choice, publish via ProvekIt." The Rust prover's verification is the substantive work; the protocol layer adds portability without changing the assurance.

## Decision summary

- **Pure-Rust, single-team, panic absence**: Kani alone.
- **Pure-Rust, single-team, functional correctness**: Prusti alone.
- **Pure-Rust, regulator/auditor-driven**: Creusot alone.
- **Polyglot, with Rust dependencies**: Kani/Prusti/Creusot + ProvekIt for federation.
- **Polyglot, with Rust libraries**: Same.

For Rust verification specifically, ProvekIt is a federation layer. The verification itself comes from one of the Rust-specific provers. The decision to add ProvekIt is independent from the choice of prover.

## Read next

- [coq-fstar-lean.md](coq-fstar-lean.md): interactive theorem provers.
- [`../../contributing/writing-a-lift-adapter/`](../../contributing/writing-a-lift-adapter/): how to lift a new prover's annotations.
- [`../../security/solver-trust.md`](../../security/solver-trust.md): TCB for different backends.
- [`../boundaries.md`](../boundaries.md): what ProvekIt is NOT.
