# Per-language status

The matrix of what's shipping, what's planned, and what's under evaluation across host languages. Updated for protocol v1.4.1 (CID `blake3-512:dc2f42ff8a4a66289cc19bfbd628898b8bd8e61d2148ecf609324cc2421c5c440a6c0e70e20ffbecabeb78e0253101d72823b7e3ab120a4d56cb67c8e31dc641`). The matrix below tracks the per-kit state under v1.4: substrate layering (envelope, header, body), contract-cid vs attestation-cid separation, contract-set extension fields, and bridge target dimensionality. Authoring-surface coverage for kit, libs, lift adapters, decorator macros, embedded verifier, CLI, and LSP plugin is unchanged from v1.1; the v1.4 readiness state is captured in the cross-kit bridge readiness matrix below. For the spec-by-spec list see [`cids.md`](cids.md).

## Component glossary

- **Kit**: per-language authoring surface. IR library, AST canonicalizer, prompt set, producer integrations, diagnostic translator. Per the kit standard at CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88`.
- **Libs**: per-language verification libraries. IR types, canonicalizer, memento envelope codec, embedded verifier.
- **Lift adapters**: per-source-library walkers that promote existing annotations to signed contract mementos.
- **Decorator macros**: per-language ergonomic surface for authoring contracts directly when no lift target exists (e.g., `#[provekit::contract]` in Rust).
- **Embedded verifier**: in-process verifier callable from the host language without spawning a subprocess.
- **CLI**: shipping command-line implementation of `prove`, `verify-protocol`, `lift`, `dump`, etc.

Legend: `+` shipping in v1.4.1 (current), `~` planned for v1.5 (next), `o` under evaluation, `-` not on roadmap.

## Matrix

| Language    | Kit | Libs | Lift adapters                                           | Decorator macros        | Embedded verifier | CLI                  | LSP Plugin           |
|-------------|-----|------|---------------------------------------------------------|--------------------------|-------------------|----------------------|----------------------|
| Rust        | `+` | `+`  | `+ proptest, contracts, kani, prusti` ; `o creusot, flux`          | `+ provekit-macros`      | `+`               | `+ provekit (canonical)` | `+`                  |
| TypeScript  | `+` | `+`  | `+ zod, class-validator, fast-check` ; `~ io-ts, valibot, ajv`     | `~`                      | `+`               | `~ (use Rust CLI)`   | `+`                  |
| Go          | `+` | `+`  | `+ go-playground/validator` ; `~ ozzo-validation`       | `~`                      | `+`               | `~ (use Rust CLI)`   | `+`                  |
| C++         | `+` | `+`  | `+ [[expects:]]/[[ensures:]] (C++26)` ; `o assert.h`    | `~ (C++26 contracts)`    | `+`               | `~ (use Rust CLI)`   | `+`                  |
| C           | `+` | `~`  | `~`                                                     | `~`                      | `~`               | `~ (use Rust CLI)`   | `+`                  |
| Zig         | `+` | `~`  | `+ provekit-lift-zig (comment annotations)`             | `~`                      | `+`               | `~ (use Rust CLI)`   | `+`                  |
| Python      | `+` | `+`  | `+ pydantic` ; `~ deal, hypothesis` ; `~ icontract, attrs`   | `+`                      | `+`               | `~ (use Rust CLI)`   | `+`                  |
| Java / JVM  | `+` | `+`  | `+ Bean Validation, JML, Spring Web, Cofoja`            | `~`                      | `~`               | `~ (use Rust CLI)`   | `~`                  |
| Ruby        | `+` | `~`  | `+ active_model, dry-validation, rspec`                 | `-`                      | `~`               | `~ (use Rust CLI)`   | `+`                  |
| C#          | `+` | `+`  | `+ DataAnnotations, Linq`                               | `+ .NET attrs`           | `+`               | `~ (use Rust CLI)`   | `+`                  |
| Swift       | `+` | `~`  | `~`                                                     | `-`                      | `~`               | `~ (use Rust CLI)`   | `+`                  |

## Cross-kit bridge readiness

This sub-matrix tracks the per-kit substrate state that supports cross-kit byte-equivalence proofs and lift-plugin-protocol bridges. The substrate guarantee depends on each kit independently asserting conformance against shared Rust contract CIDs; this table is what lets you see which kits can today.

The v1.4 substrate guarantees that depend on per-kit compliance are spread across four specs:

- The substrate-layers cut (envelope, header, body) per [`2026-05-03-substrate-layers-envelope-header-body.md`](../../protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md): every signed memento decomposes into a signed envelope, a substrate-verified header, and a verifier-opaque metadata body. Required for any tooling that wants to add body fields without growing the substrate.
- The contract-cid vs attestation-cid separation per [`2026-05-03-contract-cid-vs-attestation-cid.md`](../../protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md): each kit exposes a signer-independent `contract_cid(decl)` (or camelCase equivalent) returning the content-only CID, separate from the envelope hash returned as `attestation_cid`. Required for witness convergence across signers.
- The contract-set extension per [`2026-05-03-contract-set-extension.md`](../../protocol/specs/2026-05-03-contract-set-extension.md): self-contracts attestations carry `contractSetCid` (REQUIRED) and `previousContractSetCid` (OPTIONAL). Required for verifying semver-minor extension claims.
- The bridge target dimensionality per [`2026-05-03-bridge-target-dimensionality.md`](../../protocol/specs/2026-05-03-bridge-target-dimensionality.md): bridges emit a tagged-union `target` field (`{kind: "contract", cid}` or `{kind: "contractSet", cid}`) instead of a flat `targetContractCid`. Required for principled cross-kit bridges to contract sets vs single contracts, and to retire placeholder strings.

| Language    | Self-contracts pkg                          | Layered envelope (v1.4 §1) | contract_cid separation | contractSetCid emit | Bridge IR v1.4 (tagged-union target) | Lift-plugin-protocol bridges    | Signed attestation       |
|-------------|---------------------------------------------|----------------------------|--------------------------|---------------------|--------------------------------------|----------------------------------|--------------------------|
| Rust        | `+ provekit-self-contracts`                 | `+`                        | `+ contract_cid`         | `+`                 | `+ canonical reference (mint_bridge_v14, BridgeDeclarationV14)` | `+ source-of-truth (PR #84)`    | `+`                      |
| Go          | `+ provekit-self-contracts`                 | `~ flat universal-claim-envelope` | `+ ContractCIDFromArgs` | `+ ComputeContractSetCID` | `~ flat targetContractCid; v1.4 migration pending` | `~ Phase 2 in flight`           | `+`                      |
| TypeScript  | `+ inline (mint-ts-self-contracts)`         | `~ flat universal-claim-envelope` | `+ contractCidFromArgs` | `+ computeContractSetCid` | `~ flat targetContractCid; v1.4 migration pending` | `~ Phase 2 in flight`           | `+`                      |
| Python      | `~ via provekit-lift-py-tests`              | `not assessed`             | `not assessed`           | `not assessed`       | `~ flat targetContractCid; v1.4 migration pending` | `~ Phase 2 in flight`           | `-`                      |
| C++         | `+ provekit-self-contracts`                 | `~ flat universal-claim-envelope` | `+ contract_cid_from_args` | `+ compute_contract_set_cid` | `~ flat targetContractCid; v1.4 migration pending` | `-`                              | `+`                      |
| C           | `-`                                         | `not assessed`             | `not assessed`           | `not assessed`       | `~ v1.4 migration pending`           | `-`                              | `-`                      |
| Zig         | `-`                                         | `not assessed`             | `not assessed`           | `not assessed`       | `~ v1.4 migration pending`           | `-`                              | `-`                      |
| Java / JVM  | `-`                                         | `not assessed`             | `not assessed`           | `not assessed`       | `+ mintBridgeV14 (BridgeDeclarationV14)` | `-`                              | `-`                      |
| Ruby        | `-`                                         | `not assessed`             | `not assessed`           | `not assessed`       | `~ v1.4 migration pending`           | `-`                              | `-`                      |
| C#          | `+ Provekit.SelfContracts`                  | `~ flat universal-claim-envelope` | `+ Mint.ContractCid`     | `+ contractSetCid in attestation` | `~ flat targetContractCid; v1.4 migration pending` | `-`                              | `+`                      |
| Swift       | `-`                                         | `~ flat (no full mint pipeline)` | `not assessed (consumes rustContractCids lookup)` | `+ contractSetCid in mint-swift-self-contracts` | `~ v1.4 migration pending` | `-`                              | `-`                      |

Column meanings:

- **Layered envelope**: `+` if the kit's mint code emits the v1.4 `{envelope, header, metadata}` shape per the substrate-layers spec. `~ flat universal-claim-envelope` if the kit still emits the v1.1 universal-claim-envelope shape (`cid` plus `producerSignature` at the top level). `not assessed` where the kit has no claim-envelope mint pipeline (Python, C, Zig, Java, Ruby) and the column was not investigated against an alternative codepath.
- **contract_cid separation**: `+` plus the function name if the kit exposes a signer-independent `contract_cid(decl)` (or camelCase equivalent) per the contract-cid vs attestation-cid spec. `not assessed` where no such function was located in the implementation source.
- **contractSetCid emit**: `+` if the kit's self-contracts mint emits `contractSetCid` in the attestation. `not assessed` where no self-contracts mint was located. `previousContractSetCid` is OPTIONAL per spec; no kit currently emits it (only the protocol catalog references the field name).
- **Bridge IR v1.4 (tagged-union target)**: tracks emission of the tagged-union `target` field per [`2026-05-03-bridge-target-dimensionality.md`](../../protocol/specs/2026-05-03-bridge-target-dimensionality.md). Rust now carries the canonical reference: `BridgeDeclarationV14` is defined in `protocol/provekit-ir.cddl`, the Rust IR types are regenerated from that grammar in `implementations/rust/provekit-ir-types/src/lib.rs`, and `provekit-claim-envelope::mint_bridge_v14` emits the layered envelope/header/body shape with the tagged-union target. The conformance fixture `bridge_decl_v1_4` in `conformance/fixtures.toml` pins the canonical bytes. The v1.1 flat `mint_bridge` and the historical `bridge_decl` fixture are retained for back-compat per substrate-layers §4 (v1.1 mementos remain valid forever as historical bytes; new emissions MUST use v1.4). Other kits hold `~ flat targetContractCid; v1.4 migration pending` until they attach to the canonical reference; `~ v1.4 migration pending` is recorded for kits that emit a bridge under a kit-specific shape (Swift's `CrossKitBridges`, the C and Zig stubs). The v1.1.0 `o partial` tracker entries (Java #188 closed by Java migration; Ruby #190, C# #192, C++ #193 still pending) are unblocked by the canonical-reference PR.

## Rust (canonical reference implementation)

**Kit:** `provekit-canonicalizer`, `provekit-claim-envelope`, `provekit-proof-envelope`, `provekit-ir-symbolic`. Shipping in v1.1.

**Libs:** `provekit-verifier`, plus the kit crates above. Embedded verifier callable from any Rust crate via the public API.

**Lift adapters (shipping in v1.1):**
- `provekit-lift-proptest`: walks `proptest!` blocks. Coverage: `prop_assume!`, `prop_assert!`, `prop_assert_eq!`, `prop_assert_ne!`.
- `provekit-lift-contracts`: walks `#[requires(...)]`, `#[ensures(...)]`, `#[invariant(...)]` macros from the `contracts` crate.

**Lift adapters (shipping in v1.4.1):**
- `provekit-lift-kani`: walks `#[kani::proof]` functions, `kani::assume`, `kani::assert`.
- `provekit-lift-prusti`: walks `#[prusti_contracts::requires/ensures]`.

**Lift adapters (under evaluation):**
- `provekit-lift-creusot`: rich logical fragments; some annotations route through Tier 3 of the handshake.
- `provekit-lift-flux`: refinement types; partial lift.

**Decorator macros:** `provekit-macros` ships `#[provekit::contract]` and `#[provekit::verify]` for direct authoring when no lift target exists. The `provekit-macros-rt` crate carries the runtime support.

**Build-script integration (planned for v1.5):** `provekit-build` lifts contract violations into compile-time errors via `build.rs`. Currently in flight; see `implementations/rust/provekit-build/` and `examples/build_script_demo/`.

**Embedded verifier:** Yes. `provekit_verifier::run(project_root)` returns a `HandshakeReport` synchronously.

**CLI:** `provekit` is the canonical shipping CLI for protocol v1.4.1. Subcommands: `prove`, `verify`, `verify-protocol`, `version`, `init`, `lift`, `dump`, `hash`, `ask`, `search`, `implicate`. Distributed via `cargo install provekit`.

## TypeScript

**Kit:** Shipping in v1.1. The TypeScript kit emits the same canonical IR a Rust kit emits for the same proposition; cross-language conformance is direct.

**Libs:** `ts-types-proof` lifts TypeScript type annotations into contract mementos. Embedded verifier shipping; usable from Node and from browsers (with the WASM build of the canonicalizer).

**Lift adapters (shipping in v1.4.1):**
- `provekit-lift-zod`: walks `z.object`, `z.string`, `z.number`, validator combinators. Full chain decoder for all major zod methods.
- `provekit-lift-class-validator`: walks decorator-annotated class fields (`@IsNotEmpty`, `@MinLength`, `@Min`, `@Max`, `@IsEmail`, etc.).
- `provekit-lift-fast-check`: walks `fc.assert(fc.property(...))` blocks. Lifts property tests to `forall` contracts.

**Lift adapters (planned for v1.5):**
- `io-ts`, `runtypes`, `valibot`: validator-style schema libraries; lift logic is uniform across the family.
- `ajv` schemas: JSON Schema Draft 7+ to canonical IR.

**Decorator macros (planned for v1.5):** A `@provekit.contract(...)` decorator for direct authoring when no lift target exists.

**Embedded verifier:** Yes. Available from Node directly; browser builds use the WASM canonicalizer plus a remote prover for Tier 3 fallback.

**CLI:** Deferred to v1.5. Use the Rust CLI (`provekit prove`) for verification; the TypeScript libs handle authoring and lifting.

**LSP Plugin:** Yes. `provekit-lsp` implements the ProvekIt NDJSON LSP plugin protocol (daemon mode over stdio) with `initialize`, `parse`, and `shutdown`.

## Go

**Kit:** Shipping in v1.1. `implementations/go/provekit-ir-symbolic` provides the IR library. The canonicalizer matches the Rust implementation byte-for-byte.

**Libs:** Shipping. Embedded verifier callable from Go programs via a small CGO bridge to the canonicalizer; pure-Go verifier in flight.

**Lift adapters (shipping in v1.4.1):**
- `provekit-lift-validator`: walks `validate:` struct tags from `go-playground/validator`.

**Lift adapters (planned for v1.5):**
- `provekit-lift-ozzo`: walks `ozzo-validation` rule chains.

**Decorator macros:** Go has no decorator syntax. Comment-block annotations (`//provekit:contract`) under evaluation.

**Embedded verifier:** Yes. CGO bridge to the Rust canonicalizer; pure-Go canonicalizer planned for v1.5.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `provekit-lsp-go` implements the ProvekIt NDJSON LSP plugin protocol with `initialize`, `parse`, and `shutdown`. Scans Go source for `//provekit:` annotations.

## C++

**Kit:** Shipping in v1.1. `implementations/cpp/provekit-ir-symbolic` plus the canonicalizer. Header-only IR library; CMake integration shipped.

**Libs:** Shipping. Embedded verifier links into existing C++ projects.

**Lift adapters (shipping in v1.4.1):**
- `provekit-lift-cpp-contracts`: walks `[[expects:]]` and `[[ensures:]]` attributes (C++26 contract syntax).

**Lift adapters (under evaluation):**
- `provekit-lift-assert-h`: walks `assert(...)` macros; coverage is partial because `assert.h` discards conditional information at compile time.
- Boost.Hana metaprograms expressing type-level contracts.
- Boost.Contract pre/post/invariant annotations.

**Decorator macros:** C++26 `[[expects:]]` and `[[ensures:]]` are the native authoring surface; the lift adapter recognizes them.

**Embedded verifier:** Yes.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `provekit-lsp-cpp` implements the ProvekIt NDJSON LSP plugin protocol with `initialize`, `parse`, and `shutdown`.

## Python

**Kit:** Shipping in v1.1. `implementations/python/provekit-lift-py-tests` provides the IR library, canonicalizer (JCS + BLAKE3-512), Layer 2 lift adapter, decorator macros, Pydantic lift adapter, and embedded verifier.

**Libs:** Shipping. The canonicalizer is implemented in pure Python and is byte-identical to the Rust canonicalizer for all conformance tests. Performance is acceptable for typical project sizes; the WASM-backed path remains an option for v1.5 if profiling demands it.

**Lift adapters (shipping in v1.4.1):**
- `provekit.lift.pydantic`: walks `BaseModel` field annotations and `Field` constraints. Emits the same IR as Bean Validation `@Min`/`@Max`/`@Pattern`/`@Size` for equivalent constraints.
- Layer 2 structural lift: walks pytest/unittest test files and recognizes bounded loops, helper inlining, multi-assertion characterization, and `@pytest.mark.parametrize`.

**Lift adapters (planned for v1.5):**
- `provekit-lift-deal`: walks `@deal.pre`, `@deal.post`, `@deal.raises`.
- `provekit-lift-hypothesis`: walks `@given(...)` test functions; shape similar to proptest.
- `icontract`, `attrs`, `dataclasses-json` schemas.

**Decorator macros:** `@provekit.contract(pre=..., post=..., inv=...)` ships for direct authoring. Supports both string expressions and callable predicates (with runtime checking).

**Embedded verifier:** Yes. Delegates to the Rust `provekit` CLI via subprocess, ensuring full protocol conformance without reimplementing the verifier.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `provekit.lsp` implements the ProvekIt LSP plugin protocol (NDJSON over stdio) with `initialize`, `parse`, and `shutdown` methods.

## Java / JVM

**Kit:** Shipping in v1.1. Multi-module Maven project with SLF4J-style architecture: `provekit-lift-java-core` (facade) + per-annotation binding JARs, discovered via `java.util.ServiceLoader`.

**Libs:** Shipping. `provekit-ir` provides IR types (`Formula`, `Term`, `Sort`, `Declaration`, `IrDocument`, `CallEdgeDecl`, plus the v1.4 layered `BridgeDeclarationV14` / `BridgeHeaderV14` / `BridgeMetadataV14` / `BridgeEnvelope` / `BridgeTarget` records).

**Bridge IR v1.4:** `provekit-claim-envelope::ClaimEnvelope.mintBridgeV14` emits the layered envelope/header/body shape with the tagged-union `target` field. Byte-identical to the rust canonical reference; pinned against the `bridge_decl_v1_4` conformance fixture in `BridgeV14RoundtripTest`. The legacy v1.1 9-field flat `Declaration.Bridge` and the v1.2 layered `mintBridge` remain available for back-compat per substrate-layers spec §4 (v1.1 mementos remain valid forever as historical bytes; new emissions MUST use v1.4). Closes #188.

**Lift adapters (shipping in v1.1):**
- `provekit-lift-java-bean-validation`: walks `@NotNull`, `@Email`, `@Min`, `@Max`, `@Pattern`, `@Size`, `@Positive`, `@Negative`, `@AssertTrue`, `@AssertFalse`, `@DecimalMin`, `@DecimalMax`, `@Digits`, `@Future`, `@Past`.
- `provekit-lift-java-jml`: walks `//@ requires`, `//@ ensures`, `//@ invariant` comment-block annotations. Uses a hand-written tokenizer + recursive-descent parser (no regex gymnastics) to produce structured IR that is byte-for-byte identical to Bean Validation for equivalent constraints.
- `provekit-lift-java-spring-web`: walks `@RequestParam`, `@PathVariable`, `@RequestMapping`, etc.
- `provekit-lift-java-cofoja`: walks `@Requires`, `@Ensures`, `@Invariant` annotations.
- Plus bindings for Spring Security, Swagger, Jackson, JPA, and Hibernate annotations.

**Cross-domain equivalence:** Integration tests prove that `@NotNull`, `//@ requires x != null`, and `@RequestParam(required=true)` produce identical IR. Same for `@Min(0) @Max(100)` vs `//@ requires score >= 0 && score <= 100`.

**Decorator macros:** JVM annotations are the natural authoring surface.

**Embedded verifier:** Planned for v1.5.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Planned for v1.5.

## C

**Kit:** Shipping. `implementations/c/provekit-ir` provides the IR library, JCS canonical JSON emitter, and BLAKE3-512 hash wrapper.

**Libs:** Under evaluation. Native C BLAKE3 binding planned for v1.5; current implementation delegates hashing to the Python `blake3` module via subprocess.

**Lift adapters:** Planned for v1.5. `assert.h` macro walking under evaluation.

**Decorator macros:** Under evaluation.

**Embedded verifier:** Planned for v1.5.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `provekit-lsp-c` implements the ProvekIt NDJSON LSP plugin protocol (provekit-lsp-plugin/1 over stdio) with `initialize`, `parse`, and `shutdown`.

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

**Bridge IR gap:** `Provekit::IR.marshal_declarations` hardcodes `kind: "contract"` and cannot emit `Bridge` declarations. Originally tracked as task #190 against the v1.1.0 9-field shape; under v1.4's bridge target dimensionality spec the migration target is the layered shape with a tagged-union `target` field, so the gap is subsumed by the v1.4 migration rather than the v1.1.0 fix. Blocks Phase 2 cross-kit bridges to Rust's lift-plugin-protocol contracts.

## C#

**Kit:** Shipping in v1.1. `implementations/csharp/Provekit.IR`, `Provekit.Canonicalizer`, `Provekit.SelfContracts`, `Provekit.ClaimEnvelope`, `Provekit.ProofEnvelope`, `Provekit.Verifier`. Multi-project .NET 10 solution with full IR + canonicalizer parity to Rust.

**Libs:** Shipping. `Provekit.Verifier` is the in-process verifier.

**Lift adapters (shipping in v1.1):**
- `Provekit.Lift.DataAnnotations`: walks `[Required]`, `[StringLength]`, `[Range]`, `[RegularExpression]`, `[EmailAddress]`, etc.
- `Provekit.Lift.Linq`: walks LINQ expression trees and lifts predicate quantifiers (`All`, `Any`) to `forall`/`exists` IR.

**Decorator macros:** .NET attributes are the natural authoring surface. Lift adapters consume them directly.

**Embedded verifier:** Yes. `Provekit.Verifier` (in-process verifier) ships as part of the .NET 10 solution.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `Provekit.Lsp.Plugin` implements the ProvekIt NDJSON LSP plugin protocol.

**Bridge IR gap:** `Provekit.IR.Collector.BridgeDecl` is `(TargetContractName, IrArgSorts, IrReturnSort)`, a lift-adapter helper, not the spec Bridge shape. Originally tracked as task #192 against the v1.1.0 9-field shape; under v1.4's bridge target dimensionality spec the migration target is the layered shape with a tagged-union `target` field, so the gap is subsumed by the v1.4 migration. Self-contracts attestation IS signed (the bundle CID is pinned, and `contractSetCid` is emitted), but Phase 2 cross-kit bridges require a separate spec-shaped `BridgeDeclaration` record to be added.

## Swift

**Kit:** Shipping in v1.1 (via PR #76). `implementations/swift/Sources/Provekit/IR.swift` provides IR types, JCS canonical JSON via `Jcs.encode`, and BLAKE3-512 hashing. The conformance runner at `Sources/ConformanceRunner/main.swift` validates byte-identical emission against the canonical Rust output for `eq_atomic`, `pattern1_bounded_loop`, `contract_decl`, `bridge_decl`.

**Libs:** Under evaluation.

**Lift adapters:** Planned.

**Decorator macros:** Swift property wrappers + macros (Swift 5.9+) under evaluation as the authoring surface.

**Embedded verifier:** Planned for v1.5.

**CLI:** Deferred. Use the Rust CLI.

**LSP Plugin:** Yes. `ProveKitLSPSwift` implements the ProvekIt NDJSON LSP plugin protocol (parse-protocol v1) with `initialize`, `parse`, and `shutdown`.

**Bridge IR:** v1.1.0 9-field shape supported (`Declaration.bridge` enum case round-trips byte-identical to the bridge_decl fixture, per PR #76). Under v1.4's bridge target dimensionality spec the kit's bridge emission needs to migrate to the layered shape with a tagged-union `target` field; that migration is pending alongside the rest of the kits. Self-contracts package and Phase 2 lift-plugin-protocol bridges deferred until the kit accumulates a runtime surface beyond conformance.

## Cross-language conformance

The IR is language-agnostic. A Rust kit, a TypeScript kit, and a Go kit all emit the same canonical bytes for the same canonical formula. A contract memento minted by any kit, expressing the same proposition, shares a CID. The handshake at Tier 1 sees them as identical. This is the cross-language conformance property: a TypeScript consumer of a Rust library has the same Tier-1 discharge fraction as a Rust consumer would.

The implication for adoption: the per-language status matrix is largely about authoring ergonomics and runtime integration, not about correctness. The protocol bytes are uniform regardless of which kit produced them. A consumer in any language can verify a `.proof` produced by any kit; the canonical Rust CLI is the simplest path until per-language CLIs ship.

## How to track status

This document lives in the repository's `docs/` directory and is updated per release. The authoritative protocol catalog is at `protocol/specs/2026-04-30-protocol-catalog.json`; verify the local install conforms via `provekit verify-protocol`.

For adapter coverage of a specific source library not yet listed, the per-language kit standard (CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88`) defines the contract every adapter implements. Adapter contributions are explicitly in scope; reach out via the project repository.
