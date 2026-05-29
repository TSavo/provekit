# Thesis: hash-bounded verification

**ProvekIt is not a formal verification framework. It is a protocol for content-addressing formal verifications.**

Z3, Coq, Lean, F\*, Isabelle, Kani, Prusti, Creusot, Dafny, TLA+, CBMC: those are formal verification frameworks. They consume formulas and produce verdicts. ProvekIt does not consume formulas, does not produce verdicts. ProvekIt provides the substrate over which the verdicts of those frameworks are published, signed, distributed, federated, and composed.

This is the same shape as Bitcoin (content-addressed currency, no central mint), Git (content-addressed source history, no master copy), BitTorrent (content-addressed content distribution, no central server), IPFS (content-addressed web, no registry). Each of those took a domain that was thought to require a central authority and showed it admits a content-addressed protocol with no central party. ProvekIt applies the same primitive to behavioral verification.

ProvekIt's central claim is operational, not philosophical: a content-addressed
protocol can carry behavioral verification across a dependency graph without
making every consumer redo all prior semantic work. When a prior commitment is
unchanged, the verifier can compare CIDs, check signatures, and walk proof
edges instead of re-running the original tests or solvers.

But the deeper claim is **cross-domain proof reuse**. A proof about
JavaScript's `parseInt` can transfer to Rust's `str::parse` when both bridge to
the same reference contract. The bridge is a hash-bounded claim: "contract A
(CID X) implies contract B (CID Y)." The implication is verified once against
those bytes and can be reused by verifiers whose policy admits that memento.

The thesis breaks into two core claims, each precise, each independently checkable.

## 1. Reuse prior correctness by content identity

Modern dependency graphs span many packages and tools. A consumer should not
need to re-run every upstream test, solver, and package inspection if the
upstream claim was already minted and the bytes have not changed.

The mechanism is straightforward. A library publishes a contract memento whose
`post` formula canonicalizes to a CID. A consumer's obligation has a `pre`
formula whose canonicalization yields another CID. The handshake question is:
does `post` imply `pre`? Tier 1 answers yes when the CIDs are equal. Tier 2
answers yes when an admitted signed implication memento already carries that
edge. Tier 3 proves or rejects genuinely new edges.

The thesis is amortization. Expensive semantic work can be minted once and then
reused by content identity. New, changed, or newly composed obligations still
require semantic proof.

## 2. Reuse correctness across domains

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

The transfer is cheap after the implications `js-parseInt-v24 ->
ref-parseInt-v1` and `rust-parse-v1 -> ref-parseInt-v1` have been verified and
minted. The framework composes those edges transitively without re-running the
same solver work for every consumer.

**The hash is the boundary.** Above the hash is math: the lattice of populated formulas, the implications between them, the signatures that anchor non-repudiation. Below the hash is physics: the byte sequence that hashes to it. The verifier does not look below the hash. When the consumer's pre-hash equals the publisher's post-hash, the verifier has a proof of equality up to canonicalization, and that proof is the comparison itself.

## 3. Hash-bounded verification: the mechanism

The term "hash-bounded" means: a minted claim is referenced by the hash of its
canonical bytes, not by a tool log, package name, or mutable registry entry.
When the verifier is checking identity or a previously minted edge, the work is
bounded by content identity and the memento's verification rules, not by the
size of the original dependency graph.

More importantly, **cross-domain claims are hash-bounded too.** The bridge `A ->
B` is not a symbolic name lookup ("`parseInt` probably means this"). It is a
**content-addressed claim about content-addressed contracts.** The bridge says:
"the contract at CID X implies the contract at CID Y." The verifier checks the
edge by verifying the memento and local policy, not by interpreting names.

This is what makes supply chain attacks detectable. If an attacker injects malicious code into `lodash`, the binary CID changes, the `.proof` bundle's CID changes, the bridge's `sourceContractCid` no longer resolves, and the **build fails at compile time.** Not because someone audited the code. Because the **mathematical proof is for different bytes.**

## 4. The proof bundle travels with the package

Traditional software distribution:
```
package.json → describes the package
src/ → contains the code
node_modules/ → contains dependencies
 audits? → manual, optional, slow
```

ProvekIt proof distribution:
```
<cid>.proof -> travels with or beside the package
  ├── contracts: what the code guarantees
  ├── bridges: how it relates to other packages
  ├── binaryCid: the exact compiled artifact
  ├── members: every memento, content-addressed
  └── signature: developer-signed, tamper-evident
```

The `.proof` file does not replace native manifests or package managers. It
adds a content-addressed proof artifact beside them. Change any claimed bytes
and the CID changes; the old proof remains valid for the old bytes, while the
new bytes need new proof data.

## 5. The DAG of proofs is a verifiable execution trace

At every node in the DAG:
- `contract` = precondition → postcondition (what state change is claimed)
- `bytecode` = the EVM/Solana/WASM/native code (how state mutates)
- `evidence` = the Z3/Coq/Kani proof (why it's correct)
- `inputCids` = dependencies on prior state (what this transition requires)

Walking the DAG in topological order gives the verifier the order in which to
check claims, witnesses, signatures, policies, and implication edges. It is a
proof execution trace, not a claim that every runtime instruction in every
package has been formally verified.

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

## 9. CID equality is the hot path

At Tier 1 of the handshake, the verifier compares canonical claim CIDs. If they
match, identity discharges without theorem proving.

This is the desired hot path in a healthy ecosystem. It is not the whole proof
story. Tier 2 reuses admitted implication mementos, and Tier 3 performs
semantic proving for obligations the graph does not already carry.

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

The proof is in the bytes. The bytes are named by CID, signed by producers, and
accepted or rejected under local verifier policy.
