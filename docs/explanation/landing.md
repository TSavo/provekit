# Sugar: a proof supply chain for existing software

Modern dependency stacks, protocol stacks, and CI supply chains are deep.
Sugar turns their load-bearing claims into content-addressed proof data.

```bash
cargo install --path implementations/rust/provekit-cli
provekit verify-protocol
cd your-rust-crate
cargo provekit-lift
provekit prove
```

Install once; then start lifting native evidence into `.proof` artifacts and
proving assembled obligations.

## What it does

A library publishes signed contract mementos along with its bytes. A consumer's
verifier loads the mementos, asks kits to resolve dependency `.proof` artifacts,
walks the consumer obligations, and runs a tiered proof path: CID equality,
cached implication memento, then semantic proving for genuinely new or changed
obligations.

The same graph carries non-callsite claims too: a protocol catalog transition admitted by PEP, a proof-file consumer admitted by proof-protocol fixtures, or a generated dropper transform accepted only after re-lift.

CID equality is the cheapest path, not the whole protocol. The point is
amortization: proof work that was already minted can be reused by content
identity; new semantic obligations still need proof.

## Why it works

Verification at supply-chain scale has the same shape as currency, source history, content distribution, and the addressable web. Each was once thought to need a central authority. Each turned out to admit a content-addressed protocol with no central party. Bitcoin proved you can mint trust without a mint. Git proved a content-addressed graph holds a software project's full history. BitTorrent proved a swarm can distribute petabytes without a server. IPFS proved that "the address is the content" generalizes.

Sugar is one more application of the same primitive. The "registry" is the
BLAKE3-512 hashspace plus local policy over signed proof data. There is no
master copy. There is no service that mediates membership. There is no party
whose downtime stops the protocol. The protocol asks no one's permission to
publish; it provides bytes that verify themselves.

## Lift, don't author

Every annotation library in wide deployment already contains specifications. `proptest` invariants. `contracts` pre/post-conditions. `kani` proofs. `prusti` annotations. `pydantic` schemas. `zod` validators. `class-validator` decorators. `bean-validation` annotations. JML predicates. `go-playground/validator` tags. Each is a spec the codebase already maintains.

Sugar does not compete with these libraries. It sits beneath them. Whatever
annotation library a codebase already uses, the lift adapter promotes the
recognized annotations to content-addressed signed contract mementos, with no
rewrites and no parallel spec to maintain. Authoring stays where the developer
already is. Verification moves underneath.

The adapter surface covers Rust, TypeScript, Python, Java, Go, C#, Ruby, Zig,
C++, C, Swift, and PHP at different depths. The pattern is uniform: walk the
idiom, emit canonical IR or an extension body, mint a signed memento, and verify
or compose the claim by CID and proof evidence.

## The protocol is content-addressed

The CLI declares conformance to an embedded protocol catalog CID. Current
binaries verify that embedded catalog with:

```bash
provekit verify-protocol
```

The repository also ships a reference CID checker at `tools/recompute-spec-cids/`:

```bash
cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
```

That command re-derives catalog and spec CIDs and fails on drift.

There is no central authority that decides what a protocol version means. The bytes do.

## What ships

- A canonical Rust CLI: `provekit`. Subcommands include `prove`,
  `verify-protocol`, `proof`, `protocol`, `mint`, `dump`, `hash`, `implicate`,
  `link`, `compose`, `emit`, `materialize`, and kit-oriented gates. Bug Zoo is
  checked by the self-contained runner under `menagerie/bug-zoo/`.
- A Rust workspace of libraries: `provekit-canonicalizer`, `provekit-claim-envelope`, `provekit-proof-envelope`, `provekit-ir-symbolic`, `provekit-verifier`, `provekit-lift`, `provekit-lift-proptest`, `provekit-lift-contracts`.
- Per-language kits, verifier libs, lift adapters, and self-contract attestations.
- A protocol catalog at `protocol/specs/2026-04-30-protocol-catalog.json`, protocol extension specs, proof-protocol fixtures, and PEP evolution witnesses.

## What is active now

- PEP: protocol catalog evolution as signed, content-addressed data.
- Proof protocol: `.proof` consumer conformance fixtures and witnesses.
- Bug Zoo: executable specimens with exhibit equivalence, scoped composition checks, and fixed-pair receipts.
- GCP/TDP/ORP/CBP/FRP draft specs: extension bodies, grammar conformance, realizers, checker bytecode, and fix receipts.

## Read further

- [README.md](../../README.md) for the install path.
- [pitch.md](pitch.md) for the launch post.
- [product.md](product.md) for what Sugar replaces and complements.
- [architecture.md](architecture.md) for the four-layer model and handshake.
- [thesis.md](thesis.md) for the deeper architectural claim.
- [docs/tutorials/rust.md](../tutorials/rust.md) for the five-minute walkthrough.
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md) for the per-source-library adoption guide.
- [docs/reference/per-language-status.md](../reference/per-language-status.md) for the kit and adapter matrix.
- [protocol/specs/](../../protocol/specs/) for the canonical specs, addressed by CID.
