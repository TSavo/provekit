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

| Language    | Kit | Libs | Lift adapters                                           | Decorator macros        | Embedded verifier | CLI                  | LSP Plugin           |
|-------------|-----|------|---------------------------------------------------------|--------------------------|-------------------|----------------------|----------------------|
| Rust        | `+` | `+`  | `+ proptest, contracts` ; `~ kani, prusti` ; `o creusot, flux`     | `+ provekit-macros`      | `+`               | `+ provekit (canonical)` | `+`                  |
| TypeScript  | `+` | `+`  | `+ zod, class-validator, fast-check` ; `~ io-ts, valibot, ajv`     | `~`                      | `+`               | `~ (use Rust CLI)`   | `~`                  |
| Go          | `+` | `+`  | `~ go-playground/validator` ; `~ ozzo-validation`       | `~`                      | `+`               | `~ (use Rust CLI)`   | `~`                  |
| C++         | `+` | `+`  | `~ [[expects:]]/[[ensures:]] (C++26)` ; `o assert.h`    | `~ (C++26 contracts)`    | `+`               | `~ (use Rust CLI)`   | `~`                  |
| C           | `+` | `~`  | `~`                                                     | `~`                      | `~`               | `~ (use Rust CLI)`   | `~`                  |
| Zig         | `+` | `~`  | `~`                                                     | `~`                      | `+`               | `~ (use Rust CLI)`   | `+`                  |
| Python      | `+` | `+`  | `+ pydantic` ; `~ deal, hypothesis` ; `~ icontract, attrs`   | `+`                      | `+`               | `~ (use Rust CLI)`   | `+`                  |
| Java / JVM  | `+` | `~`  | `+ Bean Validation, JML, Spring Web, Cofoja`            | `~`                      | `~`               | `~ (use Rust CLI)`   | `~`                  |
| Ruby        | `+` | `~`  | `+ active_model, dry-validation, rspec`                 | `-`                      | `~`               | `~ (use Rust CLI)`   | `+`                  |
| C#          | `+` | `+`  | `+ DataAnnotations, Linq`                               | `+ .NET attrs`           | `~`               | `~ (use Rust CLI)`   | `+`                  |
| Swift       | `+` | `~`  | `~`                                                     | `-`                      | `~`               | `~ (use Rust CLI)`   | `~`                  |

## Cross-kit bridge readiness

This sub-matrix tracks the per-kit substrate state that supports cross-kit byte-equivalence proofs and lift-plugin-protocol bridges. The substrate guarantee depends on each kit independently asserting conformance against shared Rust contract CIDs; this table is what lets you see which kits can today.

| Language    | Self-contracts pkg                          | Bridge IR v1.1.0 (9-field) | Lift-plugin-protocol bridges    | Signed attestation       |
|-------------|---------------------------------------------|----------------------------|----------------------------------|--------------------------|
| Rust        | `+ provekit-self-contracts`                 | `+`                        | `+ source-of-truth (PR #84)`    | `+`                      |
| Go          | `+ provekit-self-contracts`                 | `+`                        | `~ Phase 2 in flight`           | `+`                      |
| TypeScript  | `+ inline (mint-ts-self-contracts)`         | `+`                        | `~ Phase 2 in flight`           | `+`                      |
| Python      | `~ via provekit-lift-py-tests`              | `+`                        | `~ Phase 2 in flight`           | `-`                      |
| C++         | `+ provekit-self-contracts`                 | `o partial; #225`          | `-`                              | `+`                      |
| C           | `-`                                         | `+`                        | `-`                              | `-`                      |
| Zig         | `-`                                         | `+`                        | `-`                              | `-`                      |
| Java / JVM  | `-`                                         | `o partial; #222`          | `-`                              | `-`                      |
| Ruby        | `-`                                         | `o partial; #223`          | `-`                              | `-`                      |
| C#          | `+ Provekit.SelfContracts`                  | `o partial; #224`          | `-`                              | `+`                      |
| Swift       | `-`                                         | `+ (PR #76)`               | `-`                              | `-`                      |

Bridge IR `o partial` means the kit currently passes the `bridge_decl` conformance fixture (the JCS bytes match) but the kit's own IR types cannot construct or round-trip the full v1.1.0 9-field Bridge. The fixture is a happy-path test; round-trip compliance is what this column tracks. Issue numbers reference tracker entries to close each gap.

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

**Lift adapters (shipping in v1.1):**
- `provekit-lift-zod`: walks `z.object`, `z.string`, `z.number`, validator combinators. Full chain decoder for all major zod methods.
- `provekit-lift-class-validator`: walks decorator-annotated class fields (`@IsNotEmpty`, `@MinLength`, `@Min`, `@Max`, `@IsEmail`, etc.).
- `provekit-lift-fast-check`: walks `fc.assert(fc.property(...))` blocks. Lifts property tests to `forall` contracts.

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

**Kit:** Shipping in v1.1. `implementations/python/provekit-lift-py-tests` provides the IR library, canonicalizer (JCS + BLAKE3-512), Layer 2 lift adapter, decorator macros, Pydantic lift adapter, and embedded verifier.

**Libs:** Shipping. The canonicalizer is implemented in pure Python and is byte-identical to the Rust canonicalizer for all conformance tests. Performance is acceptable for typical project sizes; the WASM-backed path remains an option for v1.2 if profiling demands it.

**Lift adapters (shipping in v1.1):**
- `provekit.lift.pydantic`: walks `BaseModel` field annotations and `Field` constraints. Emits the same IR as Bean Validation `@Min`/`@Max`/`@Pattern`/`@Size` for equivalent constraints.
- Layer 2 structural lift: walks pytest/unittest test files and recognizes bounded loops, helper inlining, multi-assertion characterization, and `@pytest.mark.parametrize`.

**Lift adapters (planned for v1.2):**
- `provekit-lift-deal`: walks `@deal.pre`, `@deal.post`, `@deal.raises`.
- `provekit-lift-hypothesis`: walks `@given(...)` test functions; shape similar to proptest.
- `icontract`, `attrs`, `dataclasses-json` schemas.

**Decorator macros:** `@provekit.contract(pre=..., post=..., inv=...)` ships for direct authoring. Supports both string expressions and callable predicates (with runtime checking).

**Embedded verifier:** Yes. Delegates to the Rust `provekit` CLI via subprocess, ensuring full protocol conformance without reimplementing the verifier.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `provekit.lsp` implements the ProvekIt LSP plugin protocol (NDJSON over stdio) with `initialize`, `parse`, and `shutdown` methods.

## Java / JVM

**Kit:** Shipping in v1.1. Multi-module Maven project with SLF4J-style architecture: `provekit-lift-java-core` (facade) + per-annotation binding JARs, discovered via `java.util.ServiceLoader`.

**Libs:** Planned for v1.2.

**Lift adapters (shipping in v1.1):**
- `provekit-lift-java-bean-validation`: walks `@NotNull`, `@Email`, `@Min`, `@Max`, `@Pattern`, `@Size`, `@Positive`, `@Negative`, `@AssertTrue`, `@AssertFalse`, `@DecimalMin`, `@DecimalMax`, `@Digits`, `@Future`, `@Past`.
- `provekit-lift-java-jml`: walks `//@ requires`, `//@ ensures`, `//@ invariant` comment-block annotations. Uses a hand-written tokenizer + recursive-descent parser (no regex gymnastics) to produce structured IR that is byte-for-byte identical to Bean Validation for equivalent constraints.
- `provekit-lift-java-spring-web`: walks `@RequestParam`, `@PathVariable`, `@RequestMapping`, etc.
- `provekit-lift-java-cofoja`: walks `@Requires`, `@Ensures`, `@Invariant` annotations.
- Plus bindings for Spring Security, Swagger, Jackson, JPA, and Hibernate annotations.

**Cross-domain equivalence:** Integration tests prove that `@NotNull`, `//@ requires x != null`, and `@RequestParam(required=true)` produce identical IR. Same for `@Min(0) @Max(100)` vs `//@ requires score >= 0 && score <= 100`.

**Decorator macros:** JVM annotations are the natural authoring surface.

**Embedded verifier:** Planned for v1.2.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Planned.

## C

**Kit:** Shipping in v1.1. `implementations/c/provekit-ir` provides the IR library, JCS canonical JSON emitter, and BLAKE3-512 hash wrapper.

**Libs:** Under evaluation. Native C BLAKE3 binding planned for v1.2; v1.1 delegates hashing to the Python `blake3` module via subprocess.

**Lift adapters:** Planned for v1.2. `assert.h` macro walking under evaluation.

**Decorator macros:** Under evaluation.

**Embedded verifier:** Planned for v1.2.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Planned.

## Zig

**Kit:** Shipping in v1.1. `implementations/zig/provekit-ir` provides the IR library with JCS canonical JSON serialization and BLAKE3-512 hashing via `std.crypto.blake3`.

**Libs:** Under evaluation.

**Lift adapters:** `provekit-lift-zig` scans `//provekit:contract`, `//provekit:implement`, and `//provekit:verify` annotations in Zig source files. Emits JCS canonical IR.

**Decorator macros:** Zig doesn't have attributes; comment conventions are used instead.

**Embedded verifier:** Yes. The Zig kit uses `std.crypto.blake3` natively for 64-byte XOF hashing, producing identical hashes to the Rust kit.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `provekit-lift-zig --rpc` implements the ProvekIt NDJSON LSP plugin protocol with `initialize`, `parse`, and `shutdown`.

## Ruby

**Kit:** Shipping in v1.1. `implementations/ruby/lib/provekit/ir.rb` provides IR types, JCS canonical JSON emitter, and BLAKE3-512 hashing. Requires Ruby 3+ (uses endless-method syntax); macOS system Ruby 2.6 cannot parse the kit. Conformance harness prefers Homebrew Ruby automatically.

**Libs:** Under evaluation.

**Lift adapters (shipping in v1.1):**
- `provekit/lift/active_model`: walks `validates :field, presence: true, length: { minimum: N }` declarations.
- `provekit/lift/dry_validation`: walks `Dry::Validation::Contract` rule definitions.
- `provekit/lift/rspec`: walks `RSpec.describe` blocks; lifts `it { is_expected.to ... }` matchers.

**Decorator macros:** Ruby has no native attribute syntax. Comment annotations under evaluation.

**Embedded verifier:** Planned.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `bin/provekit-lsp-ruby` implements the ProvekIt NDJSON LSP plugin protocol.

**Bridge IR gap:** `Provekit::IR.marshal_declarations` hardcodes `kind: "contract"` and cannot emit `Bridge` declarations. Tracked as task #223. Blocks Phase 2 cross-kit bridges to Rust's lift-plugin-protocol contracts.

## C#

**Kit:** Shipping in v1.1. `implementations/csharp/Provekit.IR`, `Provekit.Canonicalizer`, `Provekit.SelfContracts`, `Provekit.ClaimEnvelope`, `Provekit.ProofEnvelope`, `Provekit.Verifier`. Multi-project .NET 10 solution with full IR + canonicalizer parity to Rust.

**Libs:** Shipping. `Provekit.Verifier` is the in-process verifier.

**Lift adapters (shipping in v1.1):**
- `Provekit.Lift.DataAnnotations`: walks `[Required]`, `[StringLength]`, `[Range]`, `[RegularExpression]`, `[EmailAddress]`, etc.
- `Provekit.Lift.Linq`: walks LINQ expression trees and lifts predicate quantifiers (`All`, `Any`) to `forall`/`exists` IR.

**Decorator macros:** .NET attributes are the natural authoring surface. Lift adapters consume them directly.

**Embedded verifier:** Planned.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `Provekit.Lsp.Plugin` implements the ProvekIt NDJSON LSP plugin protocol.

**Bridge IR gap:** `Provekit.IR.Collector.BridgeDecl` is `(TargetContractName, IrArgSorts, IrReturnSort)` — a lift-adapter helper, NOT the spec v1.1.0 Bridge. Tracked as task #224. Self-contracts attestation IS signed (the bundle CID is pinned), but Phase 2 cross-kit bridges require a separate spec-shaped `BridgeDeclaration` record to be added.

## Swift

**Kit:** Shipping in v1.1 (via PR #76). `implementations/swift/Sources/Provekit/IR.swift` provides IR types, JCS canonical JSON via `Jcs.encode`, and BLAKE3-512 hashing. The conformance runner at `Sources/ConformanceRunner/main.swift` validates byte-identical emission against the canonical Rust output for `eq_atomic`, `pattern1_bounded_loop`, `contract_decl`, `bridge_decl`.

**Libs:** Under evaluation.

**Lift adapters:** Planned.

**Decorator macros:** Swift property wrappers + macros (Swift 5.9+) under evaluation as the authoring surface.

**Embedded verifier:** Planned.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Planned.

**Bridge IR:** v1.1.0 9-field shape supported (`Declaration.bridge` enum case round-trips byte-identical to the bridge_decl fixture). Self-contracts package and Phase 2 lift-plugin-protocol bridges deferred until the kit accumulates a runtime surface beyond conformance.

## Cross-language conformance

The IR is language-agnostic. A Rust kit, a TypeScript kit, and a Go kit all emit the same canonical bytes for the same canonical formula. A contract memento minted by any kit, expressing the same proposition, shares a CID. The handshake at Tier 1 sees them as identical. This is the cross-language conformance property: a TypeScript consumer of a Rust library has the same Tier-1 discharge fraction as a Rust consumer would.

The implication for adoption: the per-language status matrix is largely about authoring ergonomics and runtime integration, not about correctness. The protocol bytes are uniform regardless of which kit produced them. A consumer in any language can verify a `.proof` produced by any kit; the canonical Rust CLI is the simplest path until per-language CLIs ship.

## How to track status

This document lives in the repository's `docs/` directory and is updated per release. The authoritative protocol catalog is at `protocol/specs/2026-04-30-protocol-catalog.json`; verify the local install conforms via `provekit verify-protocol`.

For adapter coverage of a specific source library not yet listed, the per-language kit standard (CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88`) defines the contract every adapter implements. Adapter contributions are explicitly in scope; reach out via the project repository.
