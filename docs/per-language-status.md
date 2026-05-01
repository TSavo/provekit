# Per-language status

The matrix of what's shipping, what's planned, and what's under evaluation across host languages. Updated for protocol v1.1.0 (CID `blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106`).

## Component glossary

- **Kit**: per-language authoring surface. IR library, AST canonicalizer, prompt set, producer integrations, diagnostic translator. Per the kit standard at CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88`.
- **Libs**: per-language verification libraries. IR types, canonicalizer, memento envelope codec, embedded verifier.
- **Lift adapters**: per-source-library walkers that promote existing annotations to signed contract mementos.
- **Decorator macros**: per-language ergonomic surface for authoring contracts directly when no lift target exists (e.g., `#[provekit::contract]` in Rust).
- **Embedded verifier**: in-process verifier callable from the host language without spawning a subprocess.
- **CLI**: shipping command-line implementation of `prove`, `verify-protocol`, `lift`, `dump`, etc.

Legend: `+` shipping in v1.1, `~` planned for v1.2, `o` under evaluation, `-` not on roadmap.

## Matrix

| Language    | Kit | Libs | Lift adapters                                           | Decorator macros        | Embedded verifier | CLI                  |
|-------------|-----|------|---------------------------------------------------------|--------------------------|-------------------|----------------------|
| Rust        | `+` | `+`  | `+ proptest, contracts` ; `~ kani, prusti` ; `o creusot, flux`     | `+ provekit-macros`      | `+`               | `+ provekit (canonical)` |
| TypeScript  | `+` | `+`  | `~ zod, class-validator, fast-check` ; `~ io-ts, valibot, ajv`     | `~`                      | `+`               | `~ (use Rust CLI)`   |
| Go          | `+` | `+`  | `~ go-playground/validator` ; `~ ozzo-validation`       | `~`                      | `+`               | `~ (use Rust CLI)`   |
| C++         | `+` | `+`  | `~ [[expects:]]/[[ensures:]] (C++26)` ; `o assert.h`    | `~ (C++26 contracts)`    | `+`               | `~ (use Rust CLI)`   |
| Python      | `o` | `o`  | `~ pydantic, deal, hypothesis` ; `~ icontract, attrs`   | `~`                      | `o`               | `~ (use Rust CLI)`   |
| Java / JVM  | `o` | `o`  | `~ Bean Validation, JML, Cofoja`                        | `o`                      | `o`               | `~ (use Rust CLI)`   |

## Rust (canonical reference implementation)

**Kit:** `provekit-canonicalizer`, `provekit-claim-envelope`, `provekit-proof-envelope`, `provekit-ir-symbolic`. Shipping in v1.1.

**Libs:** `provekit-verifier`, plus the kit crates above. Embedded verifier callable from any Rust crate via the public API.

**Lift adapters (shipping in v1.1):**
- `provekit-lift-proptest`: walks `proptest!` blocks. Coverage: `prop_assume!`, `prop_assert!`, `prop_assert_eq!`, `prop_assert_ne!`.
- `provekit-lift-contracts`: walks `#[requires(...)]`, `#[ensures(...)]`, `#[invariant(...)]` macros from the `contracts` crate.

**Lift adapters (planned for v1.2):**
- `provekit-lift-kani`: walks `#[kani::proof]` functions, `kani::assume`, `kani::assert`.
- `provekit-lift-prusti`: walks `#[prusti_contracts::requires/ensures]`.

**Lift adapters (under evaluation):**
- `provekit-lift-creusot`: rich logical fragments; some annotations route through Tier 3 of the handshake.
- `provekit-lift-flux`: refinement types; partial lift.

**Decorator macros:** `provekit-macros` ships `#[provekit::contract]` and `#[provekit::verify]` for direct authoring when no lift target exists. The `provekit-macros-rt` crate carries the runtime support.

**Build-script integration (planned for v1.2):** `provekit-build` lifts contract violations into compile-time errors via `build.rs`. Currently in flight; see `implementations/rust/provekit-build/` and `examples/build_script_demo/`.

**Embedded verifier:** Yes. `provekit_verifier::run(project_root)` returns a `HandshakeReport` synchronously.

**CLI:** `provekit` is the canonical shipping CLI for protocol v1.1.0. Subcommands: `prove`, `verify`, `verify-protocol`, `version`, `init`, `lift`, `dump`, `hash`, `ask`, `search`, `implicate`. Distributed via `cargo install provekit`.

## TypeScript

**Kit:** Shipping in v1.1. The TypeScript kit emits the same canonical IR a Rust kit emits for the same proposition; cross-language conformance is direct.

**Libs:** `ts-types-proof` lifts TypeScript type annotations into contract mementos. Embedded verifier shipping; usable from Node and from browsers (with the WASM build of the canonicalizer).

**Lift adapters (planned for v1.2):**
- `provekit-lift-zod`: walks `z.object`, `z.string`, `z.number`, validator combinators.
- `provekit-lift-class-validator`: walks decorator-annotated class fields.
- `provekit-lift-fast-check`: walks `fc.assert(fc.property(...))` blocks.

**Lift adapters (also planned):**
- `io-ts`, `runtypes`, `valibot`: validator-style schema libraries; lift logic is uniform across the family.
- `ajv` schemas: JSON Schema Draft 7+ to canonical IR.

**Decorator macros (planned):** A `@provekit.contract(...)` decorator for direct authoring when no lift target exists.

**Embedded verifier:** Yes. Available from Node directly; browser builds use the WASM canonicalizer plus a remote prover for Tier 3 fallback.

**CLI:** Deferred to v1.2. Use the Rust CLI (`provekit prove`) for verification; the TypeScript libs handle authoring and lifting.

## Go

**Kit:** Shipping in v1.1. `implementations/go/provekit-ir-symbolic` provides the IR library. The canonicalizer matches the Rust implementation byte-for-byte.

**Libs:** Shipping. Embedded verifier callable from Go programs via a small CGO bridge to the canonicalizer; pure-Go verifier in flight.

**Lift adapters (planned for v1.2):**
- `provekit-lift-validator`: walks `validate:` struct tags from `go-playground/validator`.
- `provekit-lift-ozzo`: walks `ozzo-validation` rule chains.

**Decorator macros:** Go has no decorator syntax. Comment-block annotations (`//provekit:contract`) under evaluation.

**Embedded verifier:** Yes. CGO bridge to the Rust canonicalizer for v1.1; pure-Go canonicalizer planned for v1.2.

**CLI:** Deferred. Use the Rust CLI.

## C++

**Kit:** Shipping in v1.1. `implementations/cpp/provekit-ir-symbolic` plus the canonicalizer. Header-only IR library; CMake integration shipped.

**Libs:** Shipping. Embedded verifier links into existing C++ projects.

**Lift adapters (planned for v1.2):**
- `provekit-lift-cpp-contracts`: walks `[[expects:]]` and `[[ensures:]]` attributes (C++26 contract syntax).
- `provekit-lift-assert-h`: walks `assert(...)` macros under evaluation; coverage is partial because `assert.h` discards conditional information at compile time.

**Lift adapters (under evaluation):**
- Boost.Hana metaprograms expressing type-level contracts.
- Boost.Contract pre/post/invariant annotations.

**Decorator macros:** C++26 `[[expects:]]` and `[[ensures:]]` are the native authoring surface; the lift adapter recognizes them.

**Embedded verifier:** Yes.

**CLI:** Deferred. Use the Rust CLI.

## Python

**Kit:** Under evaluation. A Python kit shipping in v1.2 would lift `pydantic` and `deal` annotations and embed the verifier as a Python package.

**Libs:** Under evaluation. The canonicalizer is implementable in pure Python; the cost is performance (10x to 100x slower than the Rust implementation on large catalogs). A WASM build of the Rust canonicalizer is the more likely v1.2 path.

**Lift adapters (planned for v1.2):**
- `provekit-lift-pydantic`: walks `BaseModel` field annotations and `Field` constraints.
- `provekit-lift-deal`: walks `@deal.pre`, `@deal.post`, `@deal.raises`.
- `provekit-lift-hypothesis`: walks `@given(...)` test functions; shape similar to proptest.

**Lift adapters (also planned):**
- `icontract`, `attrs`, `dataclasses-json` schemas.

**Decorator macros:** A `@provekit.contract(...)` decorator for direct authoring.

**Embedded verifier:** Under evaluation; v1.2 likely ships a WASM-backed Python package.

**CLI:** Deferred. Use the Rust CLI.

## Java / JVM

**Kit:** Under evaluation. A JVM kit lifting Bean Validation, JML, and Cofoja annotations and embedding the verifier as a Maven artifact is on the v1.3 roadmap; v1.2 may ship a partial kit.

**Libs:** Under evaluation. Pure-JVM canonicalizer is feasible; performance characteristics under evaluation.

**Lift adapters (planned):**
- `provekit-lift-bean-validation`: walks `@NotNull`, `@Email`, `@Min`, `@Max`, `@Pattern`, `@Size`.
- `provekit-lift-jml`: walks `//@ requires`, `//@ ensures`, `//@ invariant` comment-block annotations.
- `provekit-lift-cofoja`: walks `@Requires` and `@Ensures` annotations.

**Decorator macros:** Under evaluation. JVM annotations are the natural authoring surface.

**Embedded verifier:** Under evaluation.

**CLI:** Deferred. Use the Rust CLI.

## Cross-language conformance

The IR is language-agnostic. A Rust kit, a TypeScript kit, and a Go kit all emit the same canonical bytes for the same canonical formula. A contract memento minted by any kit, expressing the same proposition, shares a CID. The handshake at Tier 1 sees them as identical. This is the cross-language conformance property: a TypeScript consumer of a Rust library has the same Tier-1 discharge fraction as a Rust consumer would.

The implication for adoption: the per-language status matrix is largely about authoring ergonomics and runtime integration, not about correctness. The protocol bytes are uniform regardless of which kit produced them. A consumer in any language can verify a `.proof` produced by any kit; the canonical Rust CLI is the simplest path until per-language CLIs ship.

## How to track status

This document lives in the repository's `docs/` directory and is updated per release. The authoritative protocol catalog is at `protocol/specs/2026-04-30-protocol-catalog.json`; verify the local install conforms via `provekit verify-protocol`.

For adapter coverage of a specific source library not yet listed, the per-language kit standard (CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88`) defines the contract every adapter implements. Adapter contributions are explicitly in scope; reach out via the project repository.
