# ProvekIt: verify a petabyte of behavior by comparing 64 bytes

Modern dependency stacks are deep. ProvekIt collapses them to a hash comparison.

```bash
cargo install provekit
provekit verify-protocol
cd your-rust-crate
cargo provekit-lift
provekit prove
```

Three commands. Sixty-four bytes of comparison per call site. One CPU instruction per discharge.

## What it does

A library publishes signed contract mementos along with its bytes. A consumer's verifier loads the mementos, walks every call site in the consumer's code, and runs a three-tier handshake: hash equality (free), cached implication memento (one signature verification), Z3 fallback (once per novel pair, mints the result for everyone else).

`memcmp(local, expected, 64) == 0` is the protocol. The whole stack of human-published verified knowledge, at the average case, collapses to one CPU instruction.

## Why it works

Verification at supply-chain scale has the same shape as currency, source history, content distribution, and the addressable web. Each was once thought to need a central authority. Each turned out to admit a content-addressed protocol with no central party. Bitcoin proved you can mint trust without a mint. Git proved a content-addressed graph holds a software project's full history. BitTorrent proved a swarm can distribute petabytes without a server. IPFS proved that "the address is the content" generalizes.

ProvekIt is one more application of the same primitive. The "registry" is the BLAKE3-512 hashspace. There is no master copy. There is no service that mediates membership. There is no party whose downtime stops the protocol. The protocol asks no one's permission to publish; it provides bytes that verify themselves.

## Lift, don't author

Every annotation library in wide deployment already contains specifications. `proptest` invariants. `contracts` pre/post-conditions. `kani` proofs. `prusti` annotations. `pydantic` schemas. `zod` validators. `class-validator` decorators. `bean-validation` annotations. JML predicates. `go-playground/validator` tags. Each is a spec the codebase already maintains.

ProvekIt does not compete with these libraries. It sits beneath them. Whatever annotation library a codebase already uses, the lift adapter promotes those annotations to content-addressed signed contract mementos, with no rewrites and no parallel spec to maintain. Authoring stays where the developer already is. Verification moves underneath.

The shipping adapters in v1.1 cover `proptest` and `contracts` for Rust. The v1.2 roadmap covers `kani`, `prusti`, `zod`, `class-validator`, `fast-check`, `pydantic`, `deal`, `hypothesis`, `bean-validation`, JML, Cofoja, and `go-playground/validator`. The pattern is uniform: walk the idiom, emit canonical IR, mint a signed contract memento.

## The protocol is its hash

v1.4.0 is shorthand. The canonical name of v1.4.0 is

```
blake3-512:b0f2030d56c2fddf0ecbd7032bf0344c43e30677930e3b77188fcdc4ca6325d34649e51b2efa97d6985e4be6c43173f803254a7b05fc8bf31b92eb399b60f52f
```

the BLAKE3-512 hash of the JCS-canonical form of the protocol catalog. Anyone with the spec bytes can re-derive the CID locally. The repository ships a reference implementation at `tools/recompute-spec-cids/`; `cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify` re-derives every CID and fails on any drift.

There is no central authority that decides what v1.1.0 means. The bytes do.

## What ships

- A canonical Rust CLI: `provekit`. Subcommands include `prove`, `verify-protocol`, `lift`, `dump`, `hash`, `ask`, `search`, `implicate`.
- A Rust workspace of libraries: `provekit-canonicalizer`, `provekit-claim-envelope`, `provekit-proof-envelope`, `provekit-ir-symbolic`, `provekit-verifier`, `provekit-macros`, `provekit-lift`, `provekit-lift-proptest`, `provekit-lift-contracts`.
- Per-language kits and verifier libs for TypeScript, Go, and C++.
- A protocol catalog at `protocol/specs/2026-04-30-protocol-catalog.json` and 13 spec documents, each addressed by CID.

## What's planned for v1.2

- Build-script integration (`provekit-build`): contract violations become compile-time errors in Rust.
- Stage 4 implication-server demo: the lattice tractability theorem made operational.
- Lift adapters for `kani`, `prusti`, `zod`, `class-validator`, `fast-check`, `pydantic`, `deal`, `hypothesis`, `bean-validation`, `go-playground/validator`.
- Per-language CLIs for TypeScript, Go, and Python.

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
