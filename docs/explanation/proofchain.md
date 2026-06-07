# Proofchain

A proofchain is a portable evidence structure for logically true claims.
It links canonical claims, witnesses, attestations, tool outputs, policies,
and content-addressed artifacts into a locally verifiable chain. Anyone
holding the chain can recompute the CIDs, verify the signatures, check the
witnesses, and decide whether the claim holds under their policy.

Sugar already builds proofchains. The term names the high-level primitive
formed by the existing pieces: ProofIR claims, signed mementos, `.proof`
bundles, implication witnesses, bridge attestations, and verifier policy.

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
claims back into local obligations, generated repairs, protocol migrations, or
package boundaries. Once lifted, a Rust postcondition, a TypeScript
precondition, and a protocol invariant can occupy the same logical address
space. The proofchain head carries their transitive
implications together.

## What It Contains

At the implementation layer, a proofchain is the linked evidence object that
existing Sugar artifacts already form.

| Layer | Existing artifact |
|---|---|
| Claim | Canonical ProofIR or protocol claim bytes |
| Identity | BLAKE3-512 CIDs over canonical bytes |
| Evidence step | Witness, source memento, implication memento, or bridge memento |
| Signature | Ed25519 attestation over canonical claim or envelope bytes |
| Transport | `.proof` bundle or protocol evolution artifact |
| Acceptance | Verifier policy, catalog CID, and fail-closed checks |

The `.proof` file remains the portable container format. A memento remains the
signed claim unit inside that container. A witness remains evidence for a
specific step. An attestation remains a signed statement about a claim or step.
The proofchain is the composition of those pieces into an object a verifier can
walk.

## Identity, Not Bodies

A `.proof` does not embed source code or test logs. It carries IDENTITY: CIDs,
loci, and signatures. The bodies live where the ecosystem already put them (the
package the pip/npm/cargo install shipped, a separately deployed witness
package), and they are resolved on demand and recompute-verified at the moment a
verifier needs them. This is what lets a `.proof` over a 2909-function library
stay 13M instead of carrying all of the library inside it.

Two memento kinds make this concrete.

A **SourceMemento** is a pointer plus two hashes, zero content:
`{ source_function_name, file, span, source_cid, template_cid }`. The source
already lives on disk; the `.proof` only LOCATES it (file, span) and PINS it
(source_cid, template_cid). A verifier does not ask the `.proof` for the body. It
asks the Source Oracle, whose contract is one line: given a locus plus a CID,
return the on-disk source iff it recomputes to that CID, else refuse loudly. A
tampered or wrong-version package yields a CID mismatch, the resolution refuses,
and you KNOW the on-disk source is not what was proven. This is the binary axis
of the three-axis pin made operational, checked at every resolution.
(`implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/source_oracle.py:5-19,72-83`.)

A **WitnessMemento** is a pointer plus hash plus signature, zero content:
`{ witness_cid, kind, signer, signature, runtime_cid? }`. A witness is arbitrary
content used as an attestation: a JUnit run, a program's stdout, a CI run, a
human sign-off, a poem. The substrate interprets none of it. The body lives in a
WITNESS PACKAGE (a CID-named `<cid>.witness` file), deployed separately and
pulled only by those who want to re-examine it: audit material, not ship
material. The kit oracle that resolves the body is UNTRUSTED. The Rust CLI
verifies the Ed25519 signature itself, fetches the body over RPC, BLAKE3's those
bytes itself, and compares to the pinned `witness_cid`. A body that does not
recompute is a broken oracle, caught because Rust does the math anyway. A body
that recomputes but whose honest re-run differs is drift. Both refuse, loudly,
and are distinguished.
(`implementations/rust/provekit-cli/src/witness_verify.rs:1-18`;
`implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/witness_oracle.py:1-25`.)

The rule is the same for both: trust the recomputation, never the resolver.
Exact-or-refuse, no silent loss. This is `supra omnia, rectum` made operational
at the body-resolution boundary.

## Why the Name Matters

The repo has long described the mechanics: name the proposition by CID, name
the evidence by CID, sign the edge, and fail closed when the graph does not
carry the claim. "Proofchain" names that whole thing.

The name also separates Sugar's trust model from ordinary logs and CI
statuses. A normal status says a tool reported success. A proofchain carries
the formal claims, evidence, signatures, and policies required to re-check the
result locally.

This is why a Sugar result can move across languages, repositories, package
ecosystems, CI systems, generated repairs, and time. The result is not just a
statement. It is a proofchain.

For the formal argument, see
[Substrate, Not Blockchain](../papers/03-substrate-not-blockchain.md).
