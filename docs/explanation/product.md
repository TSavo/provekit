# ProvekIt: Product

## What ProvekIt is

ProvekIt is a content-addressed verification protocol. It defines four things: a canonical IR for behavioral formulas, a signed memento envelope wrapping IR with provenance, a published `.proof` catalog of mementos addressed by CID, and a three-tier handshake algorithm that verifies a consumer's call sites against a publisher's contracts in time decoupled from the size of the dependency graph.

Verification reduces to hash comparison. When the publisher's post-condition and the consumer's pre-condition canonicalize to identical bytes, the call site is discharged for free. When they don't, a signed implication memento may exist that bridges them; the verifier checks the signature once and discharges every call site that shares the same `(post, pre)` pair. When neither path applies, Z3 runs once per novel pair, mints the result as a fresh implication memento, and every future verifier hits the cached path.

ProvekIt is shipped as a canonical Rust CLI (`provekit`) plus per-language libraries (verifier, IR, canonicalizer) for Rust, TypeScript, Go, and C++. The protocol version is itself a CID: v1.6.0 is shorthand for `blake3-512:ce04a40534986a95362d5f130fd3a1a667b7a157f0554f262af11ec7a2ac8e8b80f56c36cca93d7a180535eedc99949d760fce6ab63c405de8837fa20f00e781`. Anyone with the spec bytes can verify that label locally.

## Who it's for

Three audiences, in order of immediate fit:

**Library authors who want their behavioral guarantees to ship.** Today, a Rust crate that uses `proptest` invariants or `contracts` pre/post-conditions communicates those guarantees to whoever reads the crate's source. ProvekIt's lift adapter promotes the existing annotations to signed contract mementos that ship in a `.proof` catalog alongside the crate's bytes. Downstream consumers verify against the mementos without ever running the original test suite or invoking the original solver. The author's annotations stay where they are; the verification is now portable.

**Application teams that depend on libraries they did not write.** A consumer's verifier walks the dependency tree, loads every `.proof` it finds, and discharges call sites against the cached contract mementos. The Tier-1 hash-discharge fraction is the headline metric: a high fraction means the consumer's expectations and the library's guarantees agree on shape. A low fraction means there is real work to do, and the work is the residue, not the average case. The verifier's cost is decoupled from the depth of the dependency tree.

**Build-tool maintainers and language teams.** Per-language kits emit canonical IR. Per-language libs verify. The Rust CLI is one shipping implementation; alternative CLIs in any language are conforming as long as they accept the v1.6.0 catalog CID. The protocol is the contract; implementations are interchangeable.

## What ProvekIt replaces

Nothing. ProvekIt does not replace `cargo test`, `npm test`, `go test`, or any other test runner. It does not replace `clippy`, `eslint`, `golangci-lint`, or any other linter. It does not replace Kani, Prusti, F\*, Dafny, TLA+, or any other formal verifier.

ProvekIt replaces the absence of a portable, signed, composable substrate underneath those tools. Today, when `proptest` finds an invariant, that invariant lives in the test runner's output; nothing else can use it. When Kani proves a property, that property lives in Kani's output; nothing downstream can carry it forward. ProvekIt is the missing layer: lift the existing tool's output into a signed memento, address it by content, publish it in a `.proof`, and the next tool in the pipeline sees a cached fact instead of a fresh problem.

## What ProvekIt complements

This list is comprehensive on purpose. ProvekIt sits beneath every annotation library; it does not compete with any of them. Adoption pattern is uniform: the lift adapter walks the source library's annotations, emits canonical IR, mints a signed contract memento, and publishes.

**Rust:**
- `proptest` (lift adapter shipping in v1.1)
- `contracts` (lift adapter shipping in v1.1)
- `kani`, `prusti` (lift adapters planned for v1.2)
- `creusot`, `flux` (lift adapters under evaluation)
- `quickcheck` (idiom maps to `proptest` adapter shape)

**TypeScript / JavaScript:**
- `zod`, `class-validator`, `fast-check` (lift adapters planned for v1.2)
- `io-ts`, `runtypes`, `valibot`, `ajv` schemas (planned)
- TypeScript's own type system (the `ts-types-proof` lib lifts type annotations)

**Python:**
- `pydantic`, `attrs`, `dataclasses-json` schemas (lift adapters planned for v1.2)
- `deal`, `hypothesis`, `icontract` (planned)
- `mypy` and `pyright` annotations (planned)

**Java / JVM:**
- Bean Validation (`jakarta.validation`, `javax.validation`) (planned for v1.2)
- JML, Cofoja (planned)
- KeY-style annotations, OpenJML (planned)

**Go:**
- `go-playground/validator` (planned for v1.2)
- `ozzo-validation`, `validator.v9` (planned)
- Build-tag-based assertions (planned)

**C++:**
- C++26 contract attributes `[[expects:]]`, `[[ensures:]]` (kit shipping; lift adapter planned)
- `assert.h` patterns (planned)
- Boost.Hana and Boost.Contract (under evaluation)

The pattern is uniform across host languages. Whatever annotation library a codebase already uses, ProvekIt promotes those annotations to content-addressed signed contracts, with no rewrites and no parallel spec to maintain.

## What ProvekIt is not

ProvekIt is not a soundness-certified compliance tool. If a regulator requires output from Coq, Isabelle, F\*, or another tool whose own correctness is itself certified, those tools remain the right choice. ProvekIt's correctness rests on (a) BLAKE3-512 collision resistance, (b) Ed25519 unforgeability, (c) the underlying solver's correctness on the IR fragment used, and (d) the per-language lift adapter's faithful translation of the source library's idiom. Each of these is an honest assumption; none of them produces a regulator-accepted certificate.

ProvekIt is not a replacement for runtime testing. Tests cover concrete inputs; contracts cover the input domain. A high Tier-1 hash-discharge fraction is a strong signal that contracts compose, but adapter coverage is empirical; the per-language lift adapter only sees what it knows how to walk. Anything outside the adapter's idiom remains as untouched as it was before ProvekIt arrived.

ProvekIt is not a database. There is no central registry, no service to call, no party that decides what counts as a valid contract. The protocol asks no one's permission to publish; it provides bytes that verify themselves. The implication server, if one exists, is a passive indexer over published `.proof` files, not an authority.

ProvekIt is not a coding-agent guardrail or an LLM proof harness. The protocol does not invoke an LLM at any step. The Rust CLI invokes Z3 at Tier 3 of the handshake, and only there. Cache hits at Tier 1 and Tier 2 are network-free, solver-free, and constant-time per call site.

## Adoption surfaces

ProvekIt ships through three install paths.

**1. Library author publishes a `.proof` alongside their crate.**

```bash
cargo install provekit
cd my-crate
cargo provekit-lift   # walks proptest! and #[contracts::ensures] annotations
                      # emits target/.proof
provekit prove        # local verification of the catalog
```

The `.proof` is a signed catalog of contract mementos. Ship it alongside the crate's bytes (in `target/release/` or in the published crate, depending on the publisher's policy). Consumers find it during their own verifier walk.

**2. Application team verifies a dependency tree at build time.**

```bash
cd my-app
provekit prove
```

The verifier walks `<projectRoot>` and the dependency tree's `.proof` files, indexes the memento pool, runs the handshake at every call site, and reports the discharge breakdown. Exit code is 0 (everything discharged), 1 (violations or unresolved residue), 2 (user error), or 3 (solver unavailable / timeout).

**3. Build-script integration (planned for v1.2).**

```rust
// build.rs
fn main() {
    provekit_build::verify_or_fail();
}
```

Contract violations become compile-time errors in the same stream as type errors. ProvekIt is the proof gate, enforced at the same boundary as the type system.

## Configuration

A repository declares its conformance via a `provekit.config.yaml` at the project root:

```yaml
protocol:
  cid: blake3-512:ce04a40534986a95362d5f130fd3a1a667b7a157f0554f262af11ec7a2ac8e8b80f56c36cca93d7a180535eedc99949d760fce6ab63c405de8837fa20f00e781
  version: v1.6.0

publish:
  implications:
    target: project    # one of: local, project, registry
```

The conformance CID is the protocol version. An implementation that declares a different CID is a different protocol; implementations may declare multiple CIDs to support cross-version operation.

## What you actually get

You don't get "mathematical certainty that your code is correct." You get:

- A signed `.proof` catalog of contract mementos that ships with your library.
- A verifier that walks consumer call sites and reports the hash-discharge fraction.
- A growing lattice of cached implication mementos that amortize solver cost across the ecosystem.
- A per-call-site report identifying the residue that genuinely needs your attention.
- A protocol that does not require permission, does not need invalidation, does not call home, and does not depend on any party but the bytes you and your peers published.

The proof gate fits underneath the tools your team already runs. The compounding value comes from adoption: every published `.proof` raises the Tier-1 discharge fraction for everyone who consumes the library. Software ages backwards.
