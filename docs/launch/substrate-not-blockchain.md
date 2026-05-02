# Substrate, Not Blockchain

ProvekIt is a substrate. Blockchains are one application of it. The reverse framing, that ProvekIt is "blockchain-adjacent" or "a kind of zk-rollup tooling," gets the architecture upside down. The substrate is more general than the application, and the application is one configuration of the substrate. This essay derives that claim from first principles in five steps.

## 1. Subjective state, objective correctness

Conventional blockchains conflate two questions. The first is "what is the state of the world?" The second is "did this transition obey the rules?" Global consensus exists to settle the first so the network can verify the second. Every node holds the same ledger because the ledger IS the answer to "what happened."

ProvekIt drops the first question. State is whatever the producer says it is. There is no global ledger, no agreed-upon answer to "what's true." What ProvekIt enforces is the second question, exclusively: did the transition obey the rules?

That question turns out to be content-addressable. A `.proof` bundle either discharges its obligations against its declared contracts or it does not. The verifier walks the chain locally, recomputes hashes, checks signatures, runs the handshake against the IR formulas, and emits a fail-closed verdict. No quorum. No peer consensus. The bytes either prove what they claim or they do not.

This separation is the load-bearing move. Subjective state, objective correctness. Once you take that seriously, the next step writes itself.

## 2. The transition and the proof are the same act

If correctness is the only thing the substrate enforces, then producing a valid state transition reduces to producing a valid correctness proof. There is no third thing.

This is the Curry-Howard correspondence operating at the protocol layer. A proof of a proposition is a program that produces a witness for it. ProvekIt's `.proof` bundle is both at once: it carries the canonical IR of the transition AND the discharge mementos that prove the transition satisfies its declared contracts. The catalog memento at the root of the bundle binds the two together under one signature. Anyone who can produce a valid `.proof` has produced a valid transition. Anyone who cannot, has not.

There is no separate "execution" layer to validate against. Execution and proof collapse into one artifact. At planet scale, this means a producer in Lagos and a verifier in Tallinn never need to talk: the bundle that landed on the verifier's disk either holds up under the four substrate invariants or it does not, and the verifier's report is itself a memento that another verifier can take as evidence.

The proof IS the transition. This is what makes the next step possible.

## 3. EVM is the floor, not the ceiling

If the proof is the transition, then the substrate's only job is to enforce the SHAPE of that artifact, not its semantics. ProvekIt's discipline reduces to four invariants:

1. The bundle's bytes hash to its filename CID.
2. Every embedded member's bytes hash to its declared CID.
3. The catalog's signature verifies against its declared signer.
4. Every required member signature verifies, and the chain DAG resolves under the verifier's policy.

Those four invariants are agnostic to what is INSIDE the bundle. Member bodies are opaque byte strings under the proof-file-format grammar. Evidence variants are extensible per the universal-claim-envelope. Discharges, per the v1.4.0 binary attestation protocol, are admissible under any memento kind the envelope grammar allows.

So a member body can carry EVM bytecode. Or WASM. Or Move bytecode. Or a JSON event log. Or a SystemVerilog assertion result. Or an FDA-form attestation. Or the canonical IR of a TypeScript predicate.

```
member-body: <EVM bytecode + state delta + proof of EVM execution>
```

```
member-body: <JSON: {"action": "transfer", "from": "...", "to": "...", "amount": 100}>
```

The substrate cannot tell the difference, and does not try. A blockchain is one application of ProvekIt: a particular discipline that interprets member bodies as state-transition deltas, and a participant set that agrees on which producer's chain to follow. The substrate enforces the SHAPE of state transitions; it is normatively opaque to their SEMANTICS. EVM-on-ProvekIt is one body shape among many.

## 4. Per-producer sovereignty

Once the substrate is opaque to body semantics, there is nothing left to globally agree on. Each producer signs their own transitions. Their CID-DAG is their truth. Consumers choose which producer's chain to follow, and verifying a chain reduces to walking it under a policy and the four invariants.

This is the Cypherpunk vision realized without the consensus tax. Local trust. Proven transitions. No global coordination. If you do not like Producer A's chain, follow Producer B's. They both have to satisfy the same four invariants; beyond that, the substrate is silent on which is "real." Both are real to whoever follows them.

Sovereignty is per-producer because consensus was never doing useful work. It was doing the work of making correctness checkable when state was subjective. With proofs that travel with their transitions, the consensus layer becomes a vestigial organ.

## 5. The substrate stays small

Per-producer sovereignty plus body-opacity yields the final property: the substrate does not grow when applications do.

A new blockchain does not need a substrate change. A new consensus mechanism does not need a substrate change. A new supply-chain attestation format, a new audit-log discipline, a new ML-model provenance chain, a new sensor-telemetry envelope, a new legal-clause notarization scheme: none of them requires the substrate to ship a feature. They all slot into member bodies. The substrate stays at the four invariants.

Forward compatibility is a property of the seam between substrate and application, not a feature added to the substrate. The seam is the boundary at which the verifier stops interpreting and starts trusting the application's discipline. ProvekIt locates that seam at the body byte string. Everything below the seam is finite, signed, and frozen. Everything above the seam is unbounded.

## What this means for you

If you accept this framing, the developer move is direct: stop asking "which chain do I publish to?" and start asking "what discipline am I asserting on the bodies I sign?"

`provekit witness` is the operational surface. You build a binary, hash it, mint a `.proof` bundle whose `binaryCid` pins the artifact and whose discharges carry whatever evidence your discipline requires, then sign and ship the pair. The verifier hashes what it received, looks up the bundle, walks the chain under its policy, emits a report. The report is itself a memento another verifier can take as evidence.

You do not need a chain to do this. You do not need a network to do this. You need a producer key, a body discipline, and a consumer who agrees to walk your chain under their policy. The four invariants do the rest.

The blockchain people are doing one thing in a space that contains many. Pick your discipline.
