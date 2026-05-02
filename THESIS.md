# Thesis: hash-bounded verification

ProvekIt's central claim is operational, not philosophical: a content-addressed protocol can carry behavioral verification across an arbitrarily-deep dependency graph at a cost that does not depend on the depth of the graph or the cardinality of the address space. The verifier compares 64 bytes per call site. The 64 bytes summarize a chain of arguments whose total length is irrelevant to the comparison.

This is the same primitive that Bitcoin used for currency, that Git uses for source history, that BitTorrent uses for content distribution, that IPFS uses for the addressable web. ProvekIt is one more application: behavioral contracts as content-addressed signed mementos, composing through a lattice of cached implications.

But the deeper claim is **cross-domain verification for free**. A proof about JavaScript's `parseInt` transfers to Rust's `str::parse` because both bridge to the same reference contract. The bridge is a hash-bounded claim: "contract A (CID X) implies contract B (CID Y)." The implication is verified once, cached forever, and every verifier in every language hits the cache.

The thesis breaks into two core claims, each precise, each independently checkable.

## 1. Verify a petabyte of correctness with a hash

Modern dependency graphs span tens of thousands of packages. The total bytes a verifier could trace through transitive dependencies is, plausibly, a petabyte. ProvekIt verifies arbitrarily-deep stacks via 64-byte hash comparison.

The mechanism is straightforward. A library publishes a contract memento whose `post` formula canonicalizes to a CID. A consumer's call site has a `pre` formula whose canonicalization yields some CID. The handshake question is: does post imply pre? Tier 1 of the handshake answers yes when the CIDs are equal, by `memcmp(local, expected, 64) == 0`. One CPU instruction. Branch-free. Constant-time.

The protocol's headline arithmetic is the ratio of these two numbers: petabytes of behavior verified, 64 bytes of comparison per discharge. The ratio is unbounded above; it grows with the size of the dependency graph the consumer chose to verify.

## 2. Prove correctness across domains for free

The bridge mechanism makes cross-domain transfer automatic. When the JavaScript kit emits:

```json
{
  "kind": "bridge",
  "sourceContractCid": "bafy...js-parseInt-v24",
  "targetContractCid": "bafy...ref-parseInt-v1",
  "targetProofCid": "bafy...ecma262-v14-proof"
}
```

And the Rust kit emits:

```json
{
  "kind": "bridge",
  "sourceContractCid": "bafy...rust-parse-v1",
  "targetContractCid": "bafy...ref-parseInt-v1",
  "targetProofCid": "bafy...ecma262-v14-proof"
}
```

Both implementations bridge to the **same reference contract** (CID `bafy...ref-parseInt-v1`). A proof about JavaScript's `parseInt` transfers to Rust's `parse` because the framework walks:

```
js-parseInt-v24 → ref-parseInt-v1 ← rust-parse-v1
```

The transfer is free because the implication `js-parseInt-v24 → ref-parseInt-v1` was verified once (by the JS kit's producer) and cached. The implication `rust-parse-v1 → ref-parseInt-v1` was verified once (by the Rust kit's producer) and cached. The framework composes them transitively without re-running Z3.

**The hash is the boundary.** Above the hash is math: the lattice of populated formulas, the implications between them, the signatures that anchor non-repudiation. Below the hash is physics: the byte sequence that hashes to it. The verifier does not look below the hash. When the consumer's pre-hash equals the publisher's post-hash, the verifier has a proof of equality up to canonicalization, and that proof is the comparison itself.

## 3. Hash-bounded verification: the mechanism

The term "hash-bounded" means: the verification of a claim is bounded by the size of the hash, not by the size of the claim. A petabyte dependency graph reduces to 64 bytes per call site because the hash summarizes everything below it.

More importantly, **cross-domain claims are hash-bounded too.** The bridge `A → B` is not a symbolic name lookup ("`parseInt` probably means this"). It is a **content-addressed claim about content-addressed contracts.** The bridge says: "the contract at CID X implies the contract at CID Y." The verifier checks this by comparing hashes, not by interpreting names.

This is what makes supply chain attacks detectable. If an attacker injects malicious code into `lodash`, the binary CID changes, the `.proof` bundle's CID changes, the bridge's `sourceContractCid` no longer resolves, and the **build fails at compile time.** Not because someone audited the code. Because the **mathematical proof is for different bytes.**

## 4. The proof bundle IS the package

Traditional software distribution:
```
package.json → describes the package
src/ → contains the code
node_modules/ → contains dependencies
 audits? → manual, optional, slow
```

ProvekIt distribution:
```
<cid>.proof → IS the package
  ├── contracts: what the code guarantees
  ├── bridges: how it relates to other packages
  ├── binaryCid: the exact compiled artifact
  ├── members: every memento, content-addressed
  └── signature: developer-signed, tamper-evident
```

The `.proof` file replaces the package manifest, the lockfile, the audit report, and the SBOM. It is all of them in one content-addressed artifact. Change any bit, the CID changes, the old proof is still valid, the new one must be re-verified.

## 5. The DAG of proofs is a verifiable execution trace

At every node in the DAG:
- `contract` = precondition → postcondition (what state change is claimed)
- `bytecode` = the EVM/Solana/WASM/native code (how state mutates)
- `evidence` = the Z3/Coq/Kani proof (why it's correct)
- `inputCids` = dependencies on prior state (what this transition requires)

Walking the DAG in topological order is **executing a program where every instruction is formally verified.** The call stack IS a proof tree. The DAG tells you the exact order in which to verify theorems so that the composition is guaranteed correct.

## 6. No database; computable hashspace

The "registry" in ProvekIt is the BLAKE3-512 hashspace itself. There is no master copy. There is no service that mediates membership. There is no party whose downtime stops the protocol.

This is the lineage of Bitcoin (a global ledger with no mint), Git (a content-addressed graph with no master), BitTorrent (petabytes of content with no server), and IPFS (an addressable web with no registry). ProvekIt is one more application of the same primitive. Populated points in the hashspace are sparse: only the canonical-IR formulas that some kit has emitted exist as addresses.

## 7. Trust built into the protocol; no permission required

ProvekIt asks no party's permission to publish. The act of publishing is the act of producing bytes that verify themselves: a signed memento whose CID is its content. Anyone with a key pair can mint mementos. Anyone with the spec can verify them. The trust comes from the protocol's primitives, not from a gatekeeper.

This is the lineage of Bitcoin, BitTorrent, Tor: protocols that operate without permission because they do not need one. ProvekIt's trust model is inherited from this lineage. We don't ask anyone's permission to publish; we provide bytes that verify themselves.

## 8. Lift, don't author

Every annotation library in wide deployment already contains specifications. `proptest`, `contracts`, `kani`, `prusti`, `hypothesis`, `deal`, `pydantic`, `zod`, `class-validator`, `bean-validation`, JML, Cofoja, `go-playground/validator`. Each is an informal or semi-formal specification the codebase already maintains.

ProvekIt does not compete with these libraries. It sits beneath them. Whatever annotation library a codebase already uses, the lift adapter promotes those annotations to content-addressed signed contracts, with no rewrites and no parallel spec to maintain. Authoring stays where the developer already is. Verification moves underneath.

This is the lift-not-author posture. It is the answer to "how do we get the specifications?" that fifty years of formal methods could not solve. The specifications already exist; we just need to lift them.

## 9. The 64-byte verification is one CPU instruction

`memcmp(local, expected, 64) == 0`. Constant-time. Branch-free. The whole stack of human-published verified knowledge, at Tier 1 of the handshake, collapses to a single CPU instruction.

This is not metaphor. This is the actual wire-level instruction the verifier executes when the publisher's post-hash and the consumer's pre-hash agree. The hash is 64 bytes; the comparison is one instruction; the call site is discharged. The protocol's promise is that this is the hot path, the average case, the place where most call sites land in a healthy ecosystem.

The hash-discharge fraction (the share of call sites discharged at Tier 1 alone) is the headline metric. A high fraction means the ecosystem's contracts are composing well: publishers and consumers are agreeing on shape, and the verifier's work is amortized to near-zero.

## 10. No invalidation; provability is monotonic

Hashes are deterministic functions of canonical bytes. When bytes change, hashes change. Old implication mementos remain cryptographically valid against their stated `(antecedentHash, consequentHash)`; they simply become unreachable from any contract that has been re-canonicalized. The lattice does not need invalidation.

This is the structural absence of cache invalidation. A stale entry in a conventional cache is a poison pill; in ProvekIt, an old memento describing now-orphaned hashes neither falsifies nor poisons anything. The lattice grows monotonically. Every minted implication memento is true forever, against the bytes it was minted for.

The implication: provability is monotonic. A fact, once published, is a hash lookup forever. The protocol's value compounds with time. Software ages backwards.

## What this thesis is NOT

This is not a claim that all software bugs go away. The lift adapter sees what it knows how to walk; per-library coverage is empirical; the residue at Tier 3 of the handshake is still real work. The protocol does not turn empirical software into mathematical software. It turns one specific class of behavioral verification (the kind expressible in the IR's logical fragments) into a content-addressed substrate that composes across the dependency graph.

This is not a claim of regulator-accepted soundness. ProvekIt's correctness rests on cryptographic assumptions, the underlying solver's correctness, and per-adapter faithfulness. None of these produce a Coq-style certificate.

This is not a claim of zero adoption cost. The lift adapter is per source library; each adapter is real engineering. Today, two lift adapters exist (`proptest` and `contracts` for Rust). The roadmap covers more.

What the thesis is, is a structural claim: the verification problem at supply-chain scale has the same shape as currency, source history, content distribution, and the addressable web. Each of those problems was once thought to require a central authority. Each turned out to admit a content-addressed protocol with no central party. ProvekIt applies the same primitive to behavioral verification, and the primitive carries the load.

The proof is in the bytes. The bytes are at the CID above. The verifier is one CPU instruction.
