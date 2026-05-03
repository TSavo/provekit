# ProvekIt: Executive Summary

ProvekIt abstracts correctness into a canonical IR, allowing proof across languages.

It does this by treating verifiable propositions as content-addressed data. Every claim about behavior, every test that already passes, every contract a developer cares about, is hashed into the same sixty-four byte digest space and signed with the same Ed25519 producer key. The hash is the verification barrier. If the bytes you have hash to the CID you asked for, the bytes are the bytes the producer signed; nothing else is required.

The formal protocol specification is the bluepaper at `docs/papers/02-bluepaper.md`. This document is the executive summary: what ProvekIt is, why it matters, who it is for, and what to read next.

---

## What it is

A protocol for content-addressed verifiable propositions. Software is the launch domain because software has the deepest tooling (provers, lift adapters, agent backends), but the substrate is general: anything that can be expressed as a predicate, witnessed by a signature or a solver run, and canonicalized for hashing becomes a memento on the lattice. Hardware semantics. Scientific consensus. Legal attestations. Sensor data. Identity claims. Same envelope shape, same hash, same verification cost.

A memento is a JCS-canonical JSON object carrying a binding hash, a property hash, an evidence body, and a producer signature. Its CID is the BLAKE3-512 of its canonical bytes. A `.proof` file is a deterministic-CBOR catalog wrapping one or more mementos; its filename is its own CID. The reference implementations are the Rust peer at `implementations/rust/` and the C++ peer at `implementations/cpp/`. Both produce byte-identical outputs from the same inputs.

The lattice is the directed acyclic graph of every memento anyone has published. Edges are `inputCids`. Walking the lattice answers verification queries through three discharge tiers:

- Tier 1 is hash equality on a sixty-four byte digest (one memcmp).
- Tier 2 is a cached implication memento lookup plus an Ed25519 signature verify.
- Tier 3 is a witness from scratch (Z3, CVC5, Vampire, a notary, a lab instrument).

The empirical results in `docs/launch/showcase-results.md` measure these tiers at approximately fifty-eight nanoseconds, sixty-six microseconds, and twenty-four milliseconds respectively, against a fixture lattice of one point one million signed mementos occupying about two and a half gigabytes on disk. The cost of any single query is sixty-four bytes regardless of how big the lattice grows. The lattice tractability theorem in the bluepaper proves the asymptotic.

---

## Why it matters

Every prior verification approach has a verification cost that grows with what is being verified. Type systems type-check everything. Theorem provers discharge from the axioms. Software bills of materials enumerate every dependency. The only systems that escape this are content-addressed: Bitcoin, Git, IPFS, BitTorrent. ProvekIt extends the lineage by one rung: the trust-free system for verifiable propositions.

The architectural unlock is that any sub-DAG is a self-contained trust unit. Stop at any node; the node is your verification. Trust depth is configuration:

```toml
[verification]
trust_depth = 1            # only my own .proofs (CI default)
# trust_depth = 5          # walk through transitive deps
# trust_depth = "silicon"  # full chain to physics (medical / aerospace)
# trust_depth = "blake3-512:..."  # stop at specific anchor
```

A CI pipeline stops at its own catalog. A library author stops at the catalog they signed. A security auditor walks five hops to the OS syscall layer. A medical-device certifier walks all the way to physics. Same protocol, same memcmp, different stopping depths. The cost of verification at any chosen depth is determined by the number of nodes the user decided to walk; never by the number of nodes that exist below their stopping point.

This dissolves the perennial concern about transitive verification. You do not have to verify all the way down. You stop where your trust appetite stops. Above the anchor: math. Below the anchor: trust.

---

## Who it is for

The framework has three audiences and three CLIs that target each.

Developers writing application code in TypeScript, Rust, Python, or any language with an existing test culture get the surface plugin layer. Their existing test corpus lifts to canonical IR through one of the thirteen lift adapters that ship in the Rust workspace. A `proptest` strategy becomes a forall-quantified contract memento. A Zod schema becomes a precondition memento. A `kani` harness becomes an invariant memento. There is no migration; running `provekit lift` once mints mementos for every property the existing tests already encode. From that moment forward every passing test is a behavior witness on the lattice.

Library authors and infrastructure publishers get the catalog publish flow. A `provekit publish` step in the release pipeline bundles the project's contracts into a signed `.proof` file. Downstream consumers pull the bundle alongside the library. The library's API is now machine-verified; the consumer's call sites can be machine-verified against the library's claims; the verification is a hash compare. No coordination beyond the bytes.

Skeptical CTOs and enterprise architects get the configuration model and the trust-depth knob. Three pluggable layers (surface, agent, IR compiler) plus the trust-depth setting let an organization bind ProvekIt to its existing tooling and policy posture. A regulated environment that mandates a specific solver pins it once. A polyglot monorepo with TypeScript and Python services pins different surfaces per service while sharing one lattice. A team that prefers `cvc5` to `z3` flips one knob.

The agent layer is for the user who would rather type English than write a contract. `provekit must app.ts "users can't have negative balance"` reads the file, proposes a Zod refinement, validates it against the canonical IR grammar, mints the memento, signs, and writes the `.proof` file. The agent backend is pluggable: Claude Code, Codex, OpenCode, OpenAI, a local model behind `ollama`, a domain-specific model. The CLI surface is the same regardless of backend.

---

## The trojan horse

Most developers will never write a contract. ProvekIt does not ask them to. The lift adapters mean the framework sits beneath the annotation libraries developers already use. Zod, class-validator, Pydantic, Bean Validation, JSDoc, `[[expects:]]`, kani, prusti, proptest, hypothesis, deal, dafny, Frama-C. Every one of these is an annotation surface; the surface emits IR; the IR mints a memento. Adoption is invisible: the developer keeps writing in their preferred surface, and ProvekIt mints underneath.

The recursive consequence is that the kit verifies itself. The Rust workspace contains the kit's own contracts at `provekit-self-contracts`. Every refactor of the kit is verified against the kit's own published behavior. The framework has eaten itself; the dogfood is the demonstration.

---

## The cypherpunk lineage

Bitcoin solved double-spend without a central authority by content-addressing transactions. Git solved version control without a central authority by content-addressing commits. IPFS solved file distribution without a central authority by content-addressing content. BitTorrent solved file delivery without a central authority by content-addressing chunks.

Each system applies content-addressing to one specific kind of asset. Bitcoin: money. Git: code revisions. IPFS: files. BitTorrent: file distribution. Each ships one application of one primitive.

ProvekIt ships the substrate. Anything that can be expressed as a predicate, witnessed, and canonicalized is a memento; the memento is content-addressed; the content address is the verification barrier.

The cypherpunk thesis from the 1990s mailing lists was that cryptographic primitives compose into trust-free systems. The systems above are the proof of that thesis. ProvekIt extends the chain by one more rung: the trust-free system for verifiable propositions.

---

## Install path

The Rust kit is the reference. It is `cargo install`-able once the workspace publishes:

```sh
cargo install provekit-cli
provekit init                       # answers three questions
provekit must app.ts "claim..."     # mint your first contract
provekit verify                     # walk the lattice
```

The C++ kit is the conformance partner. The TypeScript kit is the developer-facing surface for the JavaScript ecosystem.

The launch showcase is reproducible from a clean checkout:

```sh
git checkout feat/protocol-v1-contract
cd implementations/rust
cargo build --release -p provekit-showcase
./target/release/provekit-showcase generate --size 100000 --output /tmp/showcase-lattice
./target/release/provekit-showcase benchmark --lattice /tmp/showcase-lattice --queries 10000
```

The numbers in `docs/launch/showcase-results.md` are pasted directly from such a run.

---

## What to read next

`docs/papers/02-bluepaper.md` is the formal protocol specification. Theorem statements with proofs, the canonical IR grammar in EBNF, every spec referenced by content hash, the verifier's `memcmp(buf_a, buf_b, 64) == 0` line shown verbatim. The bluepaper closes with a runnable verification: compute the catalog CID locally; if it matches the value pinned at the top, the bluepaper has just verified its own authority.

`docs/launch/showcase-results.md` is the empirical companion to the bluepaper. Real numbers from a real run on commodity hardware, every spec CID pinned, the reproduction command at the top.

`docs/launch/demo-script.md` is the five-minute walkthrough script for the launch video.

`README.md` is the developer entry point: install, run, write your first contract.

The protocol catalog itself is at `protocol/specs/2026-04-30-protocol-catalog.json`. Its content hash is the version number of the protocol; computing the hash over the bytes you have is the act of verifying you and your peer speak the same protocol.

Pick your depth. The bytes do not grow.
