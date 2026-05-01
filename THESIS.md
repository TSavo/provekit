# Thesis: the protocol is its own root of trust

ProvekIt's central claim is operational, not philosophical: a content-addressed protocol can carry behavioral verification across an arbitrarily-deep dependency graph at a cost that does not depend on the depth of the graph or the cardinality of the address space. The verifier compares 64 bytes per call site. The 64 bytes summarize a chain of arguments whose total length is irrelevant to the comparison.

This is the same primitive that Bitcoin used for currency, that Git uses for source history, that BitTorrent uses for content distribution, that IPFS uses for the addressable web. ProvekIt is one more application: behavioral contracts as content-addressed signed mementos, composing through a lattice of cached implications.

The thesis breaks into thirteen claims. Each is precise. Each is independently checkable.

## 1. The protocol version is its hash

v1.1.0 is shorthand. The canonical name of v1.1.0 is `blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106`, the BLAKE3-512 hash of the JCS-canonical form of the protocol catalog. Anyone with the spec bytes can re-derive the CID locally. The repository ships a reference implementation at `tools/recompute-spec-cids/`; `cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify` re-derives every CID and fails on any drift.

There is no central authority that decides what v1.1.0 means. The bytes do. An implementation declares conformance by quoting the CID; a verifier checks conformance by recomputing the CID. The framework eats itself: the same `proofHash` field a library uses to declare its proof-chain root is what ProvekIt uses to declare its own protocol version. ProvekIt is one more library.

## 2. The petabyte / 64-byte ratio

Modern dependency graphs span tens of thousands of packages. The total bytes a verifier could trace through transitive dependencies is, plausibly, a petabyte. ProvekIt verifies arbitrarily-deep stacks via 64-byte hash comparison.

The mechanism is straightforward. A library publishes a contract memento whose `post` formula canonicalizes to a CID. A consumer's call site has a `pre` formula whose canonicalization yields some CID. The handshake question is: does post imply pre? Tier 1 of the handshake answers yes when the CIDs are equal, by `memcmp(local, expected, 64) == 0`. One CPU instruction. Branch-free. Constant-time.

The protocol's headline arithmetic is the ratio of these two numbers: petabytes of behavior verified, 64 bytes of comparison per discharge. The ratio is unbounded above; it grows with the size of the dependency graph the consumer chose to verify.

## 3. No database; computable hashspace

The "registry" in ProvekIt is the BLAKE3-512 hashspace itself. There is no master copy. There is no service that mediates membership. There is no party whose downtime stops the protocol.

This is the lineage of Bitcoin (a global ledger with no mint), Git (a content-addressed graph with no master), BitTorrent (petabytes of content with no server), and IPFS (an addressable web with no registry). ProvekIt is one more application of the same primitive. Populated points in the hashspace are sparse: only the canonical-IR formulas that some kit has emitted exist as addresses. The math of the addressing scheme is the only common substrate.

The implication server, if one exists, is a passive indexer crawling published `.proof` files. It does not mint mementos. It does not re-sign them. It does not decide what counts as a valid contract. It indexes what publishers have already published, and serves queries against the index. Take it offline and the protocol still works; the discharge fraction at Tier 2 falls, but Tier 1 and Tier 3 continue.

## 4. Look-no-further

The hash is the verification barrier. Above the hash is math: the lattice of populated formulas, the implications between them, the signatures that anchor non-repudiation. Below the hash is physics: the byte sequence that hashes to it.

The verifier does not look below the hash. When the consumer's pre-hash equals the publisher's post-hash, the verifier has a proof of equality up to canonicalization, and that proof is the comparison itself. There is no further obligation. The hash is the diaphragm: above it, an entire lattice of reasoning; below it, a string of bytes; between them, one CPU instruction.

This is what makes the petabyte / 64-byte ratio operational. The verifier never has to traverse the petabyte. It traverses the 64 bytes.

## 5. The librarian, not the expert

The verifier is a librarian, not a theorem prover. Most of its work is content-addressed lookup over the memento pool. Tier 1 is hash equality: free. Tier 2 is signature verification on a cached implication memento: constant-time per pair. Tier 3, the only path that invokes the prover, runs once per genuinely novel `(post, pre)` pair across the ecosystem; the result is minted and the lattice grows, so the next verifier sees a Tier 2 hit.

Z3 is summoned rarely, on cache misses. Once a fact is minted, it is a hash lookup forever. The expert (the prover) is occasionally consulted; the librarian (the verifier) does most of the work. The protocol's value comes from the librarian, not the expert.

## 6. No invalidation; provability is monotonic

Hashes are deterministic functions of canonical bytes. When bytes change, hashes change. Old implication mementos remain cryptographically valid against their stated `(antecedentHash, consequentHash)`; they simply become unreachable from any contract that has been re-canonicalized. The lattice does not need invalidation.

This is the structural absence of cache invalidation. A stale entry in a conventional cache is a poison pill; in ProvekIt, an old memento describing now-orphaned hashes neither falsifies nor poisons anything. The lattice grows monotonically. Every minted implication memento is true forever, against the bytes it was minted for.

The implication: provability is monotonic. A fact, once published, is a hash lookup forever. The protocol's value compounds with time. Software ages backwards.

## 7. Search by content, not by name

Naming is brittle. A library function called `validate_email` in one package and `email_valid` in another may compute the same property; the names do not tell you. Content-addressing makes the question well-formed: "find a function whose post-condition canonicalizes to this CID" is a hash lookup over published `.proof` files.

This is the npm-by-behavior query. The hashspace is searchable; the search reduces to grep over `.proof` files. Two functions with the same behavioral guarantee, by two different authors, in two different languages, share a contract-CID and are interchangeable as dependencies of any consumer whose handshake discharges against that CID.

## 8. Trust built into the protocol; no permission required

ProvekIt asks no party's permission to publish. The act of publishing is the act of producing bytes that verify themselves: a signed memento whose CID is its content. Anyone with a key pair can mint mementos. Anyone with the spec can verify them. The trust comes from the protocol's primitives, not from a gatekeeper.

This is the lineage of Bitcoin, BitTorrent, Tor: protocols that operate without permission because they do not need one. ProvekIt's trust model is inherited from this lineage. We don't ask anyone's permission to publish; we provide bytes that verify themselves.

## 9. Lift, don't author

Every annotation library in wide deployment already contains specifications. `proptest`, `contracts`, `kani`, `prusti`, `hypothesis`, `deal`, `pydantic`, `zod`, `class-validator`, `bean-validation`, JML, Cofoja, `go-playground/validator`. Each is an informal or semi-formal specification the codebase already maintains.

ProvekIt does not compete with these libraries. It sits beneath them. Whatever annotation library a codebase already uses, the lift adapter promotes those annotations to content-addressed signed contracts, with no rewrites and no parallel spec to maintain. Authoring stays where the developer already is. Verification moves underneath.

This is the lift-not-author posture. It is the answer to "how do we get the specifications?" that fifty years of formal methods could not solve. The specifications already exist; we just need to lift them.

## 10. The 64-byte verification is one CPU instruction

`memcmp(local, expected, 64) == 0`. Constant-time. Branch-free. The whole stack of human-published verified knowledge, at Tier 1 of the handshake, collapses to a single CPU instruction.

This is not metaphor. This is the actual wire-level instruction the verifier executes when the publisher's post-hash and the consumer's pre-hash agree. The hash is 64 bytes; the comparison is one instruction; the call site is discharged. The protocol's promise is that this is the hot path, the average case, the place where most call sites land in a healthy ecosystem.

The hash-discharge fraction (the share of call sites discharged at Tier 1 alone) is the headline metric. A high fraction means the ecosystem's contracts are composing well: publishers and consumers are agreeing on shape, and the verifier's work is amortized to near-zero.

## 11. Per-language libraries plus per-language kits plus one canonical Rust CLI

Each host language has its own kit (authoring) and libs (verification, IR, canonicalizer). The Rust CLI is the canonical shipping implementation for v1.1.0. Other languages embed the verifier via libs; their CLIs are deferred to v1.2 or beyond.

The architecture is deliberately uniform. A Rust kit emits the same canonical IR a TypeScript kit emits for the same proposition. A contract minted by either kit shares a CID. A verifier in any language sees the same bytes. The protocol is the contract; implementations are interchangeable.

The matrix of kits, libs, and lift adapters is in [docs/per-language-status.md](docs/per-language-status.md).

## 12. The lattice is finite at any complexity bound

The lattice tractability theorem (CID `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07`) proves the operational claim: at any AST depth `D` and symbol-count bound `S`, the populated hashspace is finite, address computation is linear in input size, edge verification is constant-time, and adversarial cost is confined to cryptographic preimage attack.

The 2^512 cardinality of the BLAKE3-512 address space is a property of the address space, not of the search space. The search space is `L_C(G, D, S)`, the canonical-form IR values within the bound. This is finite, computable, and at small bounds enumerable. Honest verification cost is decoupled from the cryptographic security parameter; tractability rests on grammar bounds, not hash space size.

## 13. Compile-time errors for semantic violations

The Rust build-script integration (`provekit-build`, in flight, planned for v1.2) lifts contract violations into compile-time errors. The build script invokes `cargo provekit-lift` and `provekit prove`; on a non-zero exit, the build fails. The result: contract violations join the compiler's existing error stream, alongside type errors, borrow-check errors, and lint failures.

ProvekIt becomes a smarter type system extension. The proof gate fits at the same boundary the compiler already enforces, not at a later stage. Software age backwards because every codebase that adopts ProvekIt becomes easier to verify than the one shipped yesterday: the lattice of cached implications grows with adoption, the discharge fraction rises, and the residue (the per-call-site work that genuinely needs human attention) shrinks.

## What this thesis is NOT

This is not a claim that all software bugs go away. The lift adapter sees what it knows how to walk; per-library coverage is empirical; the residue at Tier 3 of the handshake is still real work. The protocol does not turn empirical software into mathematical software. It turns one specific class of behavioral verification (the kind expressible in the IR's logical fragments) into a content-addressed substrate that composes across the dependency graph.

This is not a claim of regulator-accepted soundness. ProvekIt's correctness rests on cryptographic assumptions, the underlying solver's correctness, and per-adapter faithfulness. None of these produce a Coq-style certificate.

This is not a claim of zero adoption cost. The lift adapter is per source library; each adapter is real engineering. Today, two lift adapters exist (`proptest` and `contracts` for Rust). The roadmap covers more.

What the thesis is, is a structural claim: the verification problem at supply-chain scale has the same shape as currency, source history, content distribution, and the addressable web. Each of those problems was once thought to require a central authority. Each turned out to admit a content-addressed protocol with no central party. ProvekIt applies the same primitive to behavioral verification, and the primitive carries the load.

The proof is in the bytes. The bytes are at the CID above. The verifier is one CPU instruction.
