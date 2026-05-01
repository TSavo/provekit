# provekit-build

Cargo build-script integration for ProvekIt. Wires the verifier into
`cargo build` so contract violations surface as compile-time
diagnostics instead of runtime failures.

## What it gives you

A consumer crate that opts in (see "Four-line opt-in" below) gets:

1. Every `#[provekit::contract(...)]` on a function definition is
   discovered at build time.
2. Every `#[provekit::verify]` body has its call sites enumerated and
   each site is dispatched to Z3 via a small Tier-3 obligation.
3. The verifier's findings are emitted on stable cargo channels:
   - `cargo:warning=provekit: ...` for each undischarged call site.
   - A non-zero build-script exit (cargo-equivalent of
     `compile_error!`) when `strict = true` and at least one call
     site is `Unsatisfied`.
4. A signed manifest of contracts and verify targets is hashed under
   BLAKE3-512 and written as `<cid>.proof` under the per-build
   `OUT_DIR`.
5. `cargo:rerun-if-changed=...` lines for `Cargo.toml` and every
   walked `*.rs` source file under `src/`.

## Four-line opt-in

In a consumer crate's `Cargo.toml`:

```toml
[build-dependencies]
provekit-build = "0.1"

[package.metadata.provekit]
strict = false
```

In a consumer crate's `build.rs`:

```rust
fn main() { provekit_build::run_verification(); }
```

Then annotate functions with `#[provekit::contract(...)]` and
`#[provekit::verify]` (provided by the `provekit-macros` crate);
`cargo build` runs the verifier each time the source changes.

## Configuration

The `[package.metadata.provekit]` table accepts:

| Key              | Type    | Default   | Meaning                                                          |
| ---------------- | ------- | --------- | ---------------------------------------------------------------- |
| `strict`         | bool    | `false`   | If true, undischarged call sites cause `cargo build` to fail.    |
| `mint_proof`     | bool    | `true`    | If true, write `<cid>.proof` to `OUT_DIR/provekit/`.             |
| `verify_targets` | string  | `**/*`    | Glob over `#[provekit::verify]` function names.                  |
| `z3_timeout_ms`  | integer | `3000`    | Per-call wall-clock cap for the Z3 subprocess (also `:timeout`). |

Unknown keys are rejected; defaults are intentionally permissive so a
crate without the table still gets useful diagnostics.

## Environment variables

- `PROVEKIT_Z3_PATH` — path to the `z3` binary. Defaults to `z3` on
  `$PATH`. The build script reads it once per invocation; you do not
  need to put it in `Cargo.toml`.
- `PROVEKIT_VERBOSE=1` — emit a `cargo:warning=` line for discharged
  call sites too. Off by default to keep build output quiet.

## Tier handshake

This crate implements **Tier 3** verification only: each call site is
encoded into a fresh SMT-LIB 2 script and sent to Z3 with a 3-second
timeout. Tiers 1 and 2 (handshake-cached implication mementos) are out
of scope; they live in a separate crate and depend on the
implication-store work.

The recognized post-condition shapes for v0:

- `gte(out(), num(N))` — return value is at-least N.
- `gt(out(), num(N))` — return value is strictly greater than N.
- `eq(out(), num(N))` — return value is exactly N.

Anything else classifies as `Opaque` and surfaces as `undecidable`.
The protocol's full IR is intentionally not lowered into SMT here;
that's the lifter's job.

## Verdict mapping

For each call site we emit:

```smt2
(set-option :timeout 3000)
(set-logic QF_LIA)
(declare-const out Int)
(assert <post>)             ; from the contract
(assert (= out K))          ; ONLY if surrounding `if x == K` is present
(check-sat)
```

| Surrounding `==` | Z3       | Verdict       | Note                                                |
| ---------------- | -------- | ------------- | --------------------------------------------------- |
| absent           | sat      | Discharged    | post is realizable                                  |
| absent           | unsat    | Unsatisfied   | post is itself contradictory                        |
| present (== K)   | sat      | Discharged    | branch `if x == K` is reachable under the contract  |
| present (== K)   | unsat    | Unsatisfied   | branch `if x == K` is dead per the contract         |
| any              | unknown  | Undecidable   | timed out or could not encode                       |

Strict mode keys off `Unsatisfied`.

## Why this shape

The build script protocol is the smallest stable surface cargo
exposes for tools to gate compilation. Other approaches considered:

- **Procedural macros** that emit `compile_error!` directly: viable
  for syntactic checks, but proc-macros run before macros that produce
  the actual call sites, and they cannot run an SMT solver inside the
  rustc plugin sandbox.
- **A `cargo provekit verify` subcommand** that the user runs by
  hand: leaves the wall between code and contract intact; the user
  can forget. The build-script path makes `cargo build` itself the
  enforcement boundary.
- **Generating + linking + running an extractor binary** to call
  `inventory::iter::<ContractRegistration>()` against the consumer's
  lib: heavier, requires the lib to compile before verification, and
  buys nothing the source-walk doesn't already give.

The source-walk is the v0 path. A follow-up task can add a
post-link extractor for richer formula vocabularies that the syn-walk
cannot lift.

## Public API

- `run_verification()` — entry point for `build.rs`.
- `run_verification_inner(manifest_dir, cargo_toml, out_dir, cfg)` —
  programmatic entry point for tests.
- `ProvekitConfig`, `parse_config_from_str`, `parse_config_from_path`
  — config parsing.
- `mint_proof_file(target_dir, walk)` — produce `<cid>.proof`.
- `solve(z3_path, smt2_script, timeout_ms)` — wall-clock-guarded
  Z3 subprocess driver.
- `build_obligation_script(cfg, label, post, surrounding_eq)` — emit
  the SMT-LIB 2 source for one call site.
- `source_walk::walk(manifest_dir)` — syn-based source walker. Public
  so other build-script integrations can re-use the visitor.

## Tests

`cargo test --release -p provekit-build` runs 14 integration tests
covering config parsing (3), strict-mode plumbing (1), mint_proof
flag and determinism (2), solver subprocess (2), and source-walker
shape recognition (6).
