# ProvekIt: prove software correctness across domains by comparing 64 bytes

Modern dependency stacks, protocol stacks, and CI supply chains are deep. ProvekIt collapses their load-bearing claims to content-addressed evidence.

```bash
cargo install --path implementations/rust/provekit-cli
provekit verify-protocol
cd your-rust-crate
cargo provekit-lift
provekit prove
```

Install once; then three commands. Sixty-four bytes of comparison per call site. One CPU instruction per discharge.

## What it does

A library publishes signed contract mementos along with its bytes. A consumer's verifier loads the mementos, walks every call site in the consumer's code, and runs a three-tier handshake: hash equality (free), cached implication memento (one signature verification), Z3 fallback (once per novel pair, mints the result for everyone else).

The same graph carries non-callsite claims too: a protocol catalog transition admitted by PEP, a proof-file consumer admitted by proof-protocol fixtures, a CI result bound to a CICP blast radius, or a generated dropper transform accepted only after re-lift.

`memcmp(local, expected, 64) == 0` is the protocol. The whole stack of human-published verified knowledge, at the average case, collapses to one CPU instruction.

## Why it works

Verification at supply-chain scale has the same shape as currency, source history, content distribution, and the addressable web. Each was once thought to need a central authority. Each turned out to admit a content-addressed protocol with no central party. Bitcoin proved you can mint trust without a mint. Git proved a content-addressed graph holds a software project's full history. BitTorrent proved a swarm can distribute petabytes without a server. IPFS proved that "the address is the content" generalizes.

ProvekIt is one more application of the same primitive. The "registry" is the BLAKE3-512 hashspace. There is no master copy. There is no service that mediates membership. There is no party whose downtime stops the protocol. The protocol asks no one's permission to publish; it provides bytes that verify themselves.

## Lift, don't author

Every annotation library in wide deployment already contains specifications. `proptest` invariants. `contracts` pre/post-conditions. `kani` proofs. `prusti` annotations. `pydantic` schemas. `zod` validators. `class-validator` decorators. `bean-validation` annotations. JML predicates. `go-playground/validator` tags. Each is a spec the codebase already maintains.

ProvekIt does not compete with these libraries. It sits beneath them. Whatever annotation library a codebase already uses, the lift adapter promotes those annotations to content-addressed signed contract mementos, with no rewrites and no parallel spec to maintain. Authoring stays where the developer already is. Verification moves underneath.

The adapter surface covers Rust, TypeScript, Python, Java, Go, C#, Ruby, Zig, C++, C, Swift, and PHP at different depths. The pattern is uniform: walk the idiom, emit canonical IR or an extension body, mint a signed memento, and verify the claim by CID.

## The protocol is its hash

v1.6.2 is shorthand. The canonical name of v1.6.2 is

```
blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f
```

the BLAKE3-512 hash of the JCS-canonical form of the protocol catalog. Anyone with the spec bytes can re-derive the CID locally. The repository ships a reference implementation at `tools/recompute-spec-cids/`; `cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify` re-derives every CID and fails on any drift.

There is no central authority that decides what a protocol version means. The bytes do.

## What ships

- A canonical Rust CLI: `provekit`. Subcommands include `prove`, `verify-protocol`, `proof`, `protocol`, `ci`, `mint`, `dump`, `hash`, `ask`, `search`, `implicate`. Bug Zoo is checked by the self-contained runner under `bug-zoo/`.
- A Rust workspace of libraries: `provekit-canonicalizer`, `provekit-claim-envelope`, `provekit-proof-envelope`, `provekit-ir-symbolic`, `provekit-verifier`, `provekit-macros`, `provekit-lift`, `provekit-lift-proptest`, `provekit-lift-contracts`.
- Per-language kits, verifier libs, lift adapters, CICP vector checks, and self-contract attestations.
- A protocol catalog at `protocol/specs/2026-04-30-protocol-catalog.json`, protocol extension specs, proof-protocol fixtures, CICP vectors, and PEP evolution witnesses.

## What is active now

- PEP: protocol catalog evolution as signed, content-addressed data.
- CICP: CI results bound to source/protocol/toolchain/config/witness closures.
- Proof protocol: `.proof` consumer conformance fixtures and witnesses.
- Bug Zoo: executable specimens with exhibit equivalence, scoped composition checks, and fixed-pair receipts.
- GCP/TDP/ORP/CBP/FRP draft specs: extension bodies, grammar conformance, realizers, checker bytecode, and fix receipts.

## Read further

- [README.md](../../README.md) for the install path.
- [pitch.md](pitch.md) for the launch post.
- [product.md](product.md) for what ProvekIt replaces and complements.
- [architecture.md](architecture.md) for the four-layer model and handshake.
- [thesis.md](thesis.md) for the deeper architectural claim.
- [docs/tutorials/rust.md](../tutorials/rust.md) for the five-minute walkthrough.
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md) for the per-source-library adoption guide.
- [docs/reference/per-language-status.md](../reference/per-language-status.md) for the kit and adapter matrix.
- [protocol/specs/](../../protocol/specs/) for the canonical specs, addressed by CID.
