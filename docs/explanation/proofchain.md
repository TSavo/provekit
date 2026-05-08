# Proofchain

A proofchain is a portable evidence structure for logically true claims.
It links canonical claims, witnesses, attestations, tool outputs, policies,
and content-addressed artifacts into a locally verifiable chain. Anyone
holding the chain can recompute the CIDs, verify the signatures, check the
witnesses, and decide whether the claim holds under their policy.

ProvekIt already builds proofchains. The term names the high-level primitive
formed by the existing pieces: ProofIR claims, signed mementos, `.proof`
bundles, implication witnesses, bridge attestations, CICP result witnesses,
and verifier policy.

## The Payload

Structurally, a proofchain feels like a blockchain, but without the overhead
of distributed consensus, because proof validity does not need a global
ledger. Logically true claims need portable, signed, content-addressed
evidence.

The difference is in the payload. The payload of a blockchain is a series of
state transitions. The payload of a proofchain is a series of formal proofs.
That shift is slight, but fundamental: the chain is no longer establishing
what a network agreed happened; it is carrying why a claim is true.

If strict public ordering matters, publish a `.proof` CID or proofchain head
CID on a blockchain. That adds an ordered publication witness. It does not make
the proof valid; the proofchain still verifies locally from its own bytes.

The head has the same primitive force. A blockchain head carries the state
consequences of all prior blocks. A proofchain head carries the implication
closure of all prior proofs under verifier policy. If a proofchain contains
`p -> q` and `q -> r`, and policy admits implication composition, the head
carries `p -> r`. The commitment math is the same: hash links, Merkle roots,
signatures, and deterministic local verification. The payload is different.

ProofIR and lifters/lowerers make that closure cross boundaries. A lifter turns
a host artifact into canonical claims; a lowerer or adapter maps admitted
claims back into local obligations, generated repairs, protocol migrations, CI
closures, or package boundaries. Once lifted, a Rust postcondition, a
TypeScript precondition, a protocol invariant, and a CI witness can occupy the
same logical address space. The proofchain head carries their transitive
implications together.

## What It Contains

At the implementation layer, a proofchain is the linked evidence object that
existing ProvekIt artifacts already form.

| Layer | Existing artifact |
|---|---|
| Claim | Canonical ProofIR or protocol claim bytes |
| Identity | BLAKE3-512 CIDs over canonical bytes |
| Evidence step | Witness, implication memento, bridge memento, or CI result witness |
| Signature | Ed25519 attestation over canonical claim or envelope bytes |
| Transport | `.proof` bundle, checked-in CICP witness, or protocol evolution artifact |
| Acceptance | Verifier policy, catalog CID, and fail-closed checks |

The `.proof` file remains the portable container format. A memento remains the
signed claim unit inside that container. A witness remains evidence for a
specific step. An attestation remains a signed statement about a claim or step.
The proofchain is the composition of those pieces into an object a verifier can
walk.

## Why the Name Matters

The repo has long described the mechanics: name the proposition by CID, name
the evidence by CID, sign the edge, and fail closed when the graph does not
carry the claim. "Proofchain" names that whole thing.

The name also separates ProvekIt's trust model from ordinary logs and CI
statuses. A normal status says a tool reported success. A proofchain carries
the formal claims, evidence, signatures, and policies required to re-check the
result locally.

This is why a ProvekIt result can move across languages, repositories, package
ecosystems, CI systems, generated repairs, and time. The result is not just a
statement. It is a proofchain.

For the formal argument, see
[Substrate, Not Blockchain](../papers/03-substrate-not-blockchain.md).
