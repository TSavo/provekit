# Build-script integration spec

Status: v0, 2026-04-30. Not promoted to `protocol/specs/` yet because
this layer is implementation-shaped, not protocol-shaped: it tells
ProvekIt's Rust kit how to ride cargo's stable build-script protocol,
but every other peer implementation (C++, Go, TS) gets a different
ride that suits its own build system. The spec lives here next to the
crate; if a second-language consumer needs the same shape we'll lift
it into `protocol/specs/` then.

## Cargo's build-script protocol, abridged

Cargo executes a crate's `build.rs` once before compiling the crate
itself. The build script can communicate with cargo by writing
`cargo:KEY=VALUE` lines to its stdout. The handful that matter for us:

- `cargo:rerun-if-changed=<path>`: declare an input dependency.
  Cargo skips the script next build if no listed path has changed.
- `cargo:warning=<text>`: emit a warning that cargo prints with the
  build output. We use this for every undischarged call site.

A non-zero exit from the build script is treated as a build failure;
we use this in strict mode as the equivalent of `compile_error!`.

Reference: <https://doc.rust-lang.org/cargo/reference/build-scripts.html>.

## Pipeline

```
                  ┌─────────────────┐
   Cargo.toml ──▶ │ parse_config    │
                  └─────────────────┘
                         │ ProvekitConfig
                         ▼
                  ┌─────────────────┐
   src/**/*.rs ─▶ │ source_walk     │
                  └─────────────────┘
                         │ WalkOutcome { contracts, verify_targets, callsites }
                ┌────────┴────────┐
                ▼                 ▼
       ┌──────────────┐    ┌────────────────┐
       │ mint_proof   │    │ solve (per cs) │
       └──────────────┘    └────────────────┘
                │                 │
                │                 │ SolverVerdict per call site
                ▼                 ▼
       <cid>.proof          VerificationReport
                                  │
                                  ▼
                        cargo:warning= lines
                        process exit code
```

## Why source-walk and not inventory iteration

`provekit-macros` registers contracts and verify targets via
`inventory::submit!`. That mechanism populates a distributed slice
inside the consumer crate's compiled artifact. The build script runs
in a separate process *before* the consumer compiles, so it cannot
call `inventory::iter::<ContractRegistration>()`. Two paths around it:

1. **Source-walk** the consumer's `src/` with `syn` to find the
   attribute annotations directly.
2. **Generate-and-run** an extractor binary that links the consumer
   lib and prints registrations.

For v0 we picked source-walk because it's strictly simpler, gives us
source paths and line numbers for free, and matches the granularity
the verifier needs (per-call-site obligations require per-body
inspection anyway).

The cost: post-condition formulas are token streams the build script
cannot evaluate. v0 recognizes a tiny pattern set (`gte`/`gt`/`eq`
against a numeric literal); everything else falls to `Opaque`.

## Tier handshake

Three tiers govern how an obligation gets discharged:

- **Tier 1**: lookup by content hash against a cached implication
  store. Cheapest; out of scope here.
- **Tier 2**: handshake-cached implication mementos. Slightly
  more expensive; out of scope here.
- **Tier 3**: per-callsite SMT round-trip via Z3. Implemented here.

The build-script integration only does Tier 3 in v0. When a higher
tier exists in the consumer's `.proof` cache, the verifier will
short-circuit at that tier; right now there's no cache, so every
call site goes to Z3.

## SMT-LIB encoding

For each `(callsite, contract)` pair we emit:

```smt2
(set-option :timeout 3000)
(set-logic QF_LIA)
(declare-const out Int)
(assert <post>)
(assert (= out K))    ; if surrounding `if x == K` was detected
(check-sat)
```

The first non-empty line of Z3's stdout determines the verdict
(`unsat` / `sat` / `unknown` / other).

Wall-clock guard: even when Z3 honors `:timeout`, the parent loops on
`try_wait` against a deadline of `z3_timeout_ms` and SIGKILLs the
child on overrun. Both are needed: a hung Z3 binary that never reads
its options would otherwise wedge the build forever.

## Determinism

The minted `.proof` is BLAKE3-512 of a JSON manifest of (sorted)
contracts + verify targets. Same source tree → same CID. The test
`mint_proof_is_deterministic` asserts this.

## Failure modes and what they look like

| Situation                     | Surfaces as                                      | Fails build? |
| ----------------------------- | ------------------------------------------------ | ------------ |
| Z3 missing                    | `cargo:warning=... SKIP ... spawn z3: ...`       | only strict + Unsatisfied |
| Z3 timeout                    | `cargo:warning=... SKIP ... unknown / wall-clock`| no           |
| Post-condition opaque         | `cargo:warning=... SKIP ... post=opaque`         | no           |
| Discharged call site          | silent (or with `PROVEKIT_VERBOSE=1`)            | no           |
| Unsatisfied call site, lax    | `cargo:warning=... WARN ...`                     | no           |
| Unsatisfied call site, strict | `cargo:warning=... ERROR ...` + exit 1           | yes          |
| Cargo.toml unknown key        | build-script stderr, build proceeds              | no           |

## Open questions for v1

- Lift the source-walker into a separate kit-aware crate so the
  formula vocabulary can grow without bloating `provekit-build`.
- Add a `[package.metadata.provekit.cache]` table pointing at a
  shared implication-store directory so Tiers 1/2 can short-circuit
  most call sites without invoking Z3 at all.
- Wire `cargo:rerun-if-env-changed=PROVEKIT_Z3_PATH` so cache
  invalidation tracks the solver path.
