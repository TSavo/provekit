# Substrate, Not Blockchain

ProvekIt is a substrate. Blockchains are one application of it. The reverse framing, that ProvekIt is "blockchain-adjacent" or "a kind of zk-rollup tooling," gets the architecture upside down. The substrate is more general than the application, and the application is one configuration of the substrate. This essay derives that claim from first principles in six steps.

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

## 4. The DAG is a tape

"One body shape among many" is not a marketing claim. It is a structural ceiling. The reason the substrate hosts EVM, WASM, Move, JSON deltas, attestations, and sensor envelopes interchangeably is that the four invariants plus the witness DAG already constitute a verified universal computer. There is no shape above this one to reach for.

Walk the equivalence directly. A Turing machine is a transition rule, a head, a tape, and a sequence. ProvekIt has all four:

```
(transition rule, head, tape, sequence) = (four invariants, witnesses, member bodies, DAG)
```

Witnesses commit each successor to its predecessors via CID. That is the head and the sequence: any node in the DAG knows its full causal past, and the order of work is fixed by content addressing rather than by clock. The four invariants are the transition rule: every step is admitted or rejected by the same finite, fail-closed check. Member bodies are the tape: freeform bytes under the proof-file-format grammar, hence a universal alphabet. State transitions plus DAG ordering plus a transition rule is the structure of a Turing machine, and ProvekIt is, structurally, a verified universal computer.

Once you accept that, "what should the substrate add next?" becomes a malformed question. There is nothing above Turing complete. Anyone proposing a more general substrate is either building a less general one (a special-purpose chain, a fixed VM, a typed but narrower envelope) or reinventing the same four invariants under a different brand. The design space above the substrate is full of applications. The space at the substrate level is small because it has to be. This is why §3's "EVM is the floor" is structural rather than aspirational. The ceiling is theorem, not promise.

Conventional blockchains pick one tape format (state-machine deltas) and one consensus discipline (totally ordered chain over a globally shared ledger). That is a restriction on the substrate, not an extension. They left generality on the floor to buy something they did not actually need (consensus on subjective state) for the thing they actually wanted (verifiable transitions). Strip the consensus, keep the verification. You get more, not less. A Turing complete substrate already contains every program a smart contract could embed; the substrate is the bigger set.

## 5. Per-producer sovereignty

Once the substrate is opaque to body semantics, there is nothing left to globally agree on. Each producer signs their own transitions. Their CID-DAG is their truth. Consumers choose which producer's chain to follow, and verifying a chain reduces to walking it under a policy and the four invariants.

This is the Cypherpunk vision realized without the consensus tax. Local trust. Proven transitions. No global coordination. If you do not like Producer A's chain, follow Producer B's. They both have to satisfy the same four invariants; beyond that, the substrate is silent on which is "real." Both are real to whoever follows them.

Sovereignty is per-producer because consensus was never doing useful work. It was doing the work of making correctness checkable when state was subjective. With proofs that travel with their transitions, the consensus layer becomes a vestigial organ.

## 6. The substrate stays small

Per-producer sovereignty plus body-opacity yields the final property: the substrate does not grow when applications do.

A new blockchain does not need a substrate change. A new consensus mechanism does not need a substrate change. A new supply-chain attestation format, a new audit-log discipline, a new ML-model provenance chain, a new sensor-telemetry envelope, a new legal-clause notarization scheme: none of them requires the substrate to ship a feature. They all slot into member bodies. The substrate stays at the four invariants.

Forward compatibility is a property of the seam between substrate and application, not a feature added to the substrate. The seam is the boundary at which the verifier stops interpreting and starts trusting the application's discipline. ProvekIt locates that seam at the body byte string. Everything below the seam is finite, signed, and frozen. Everything above the seam is unbounded.

## 7. DAGs form witness chains

The value of a content-addressed DAG of signed attestations is that endorsements compose.

A signed attestation binds `(binaryCid, contractCid, signerPubkey)` under a signature. That is a single cryptographic claim. The next attestation can reference the previous one as evidence: signer Bob can attest that he has reviewed Alice's attestation and concurs. Charlie can attest that he has reviewed Bob's. Each new link is its own independent claim under its own key, but the chain transitively endorses the original contract.

This is the witness chain. It is not a feature added to the substrate. It emerges from the DAG: any node can be the target of a future signed attestation, so any claim can be witnessed, and any witness can be witnessed. Chains grow by composition, not by central coordination. Multiple chains can converge on the same contract from different signers, different times, different organizations. The auditor does not have to trust any single signer; they walk the DAG until they find a path through signers they do trust to the claim they care about.

Trust is local and pluggable. If you trust Bob, his attestation that Alice's contract is well-formed is evidence for you. If you trust Charlie, his attestation that he reviewed Bob's review extends the chain. The substrate does not pick a path for you; it gives you the graph and lets you walk it under your own policy.

Walking is cheap. The same property that makes content addressing work for individual claims makes it work for chains of them: validating a witness chain of length N is N hash comparisons, each comparing two 64-byte BLAKE3-512 digests. Constant work per link, linear in chain length, no replay of prior history required. A chain ten thousand witnesses deep validates in milliseconds on commodity hardware. The cost of chains does not grow with their depth; it grows with their length, at the speed of hashing.

This is the structural advantage over consensus systems. A blockchain validator must replay state transitions to check a block; depth is expensive. A substrate verifier hash-compares each link; depth is free. Witness composition therefore scales: you can build chains as deep as your evidence requires, and the auditor pays only for the walk, not for the history under each node.

A corollary: derived views are free. "Who has witnessed contract C?" is a DAG walk.

```
witnesses(contract_cid, snapshot) :=
    walk(snapshot)
        .filter(attestation.target_contract_cid == contract_cid)
        .collect()
```

The result is content-addressable but not a stored artifact. Anyone can run the walk, anyone can republish the result, no curator's set is more authoritative than another's. You do not need a gatherer. The DAG plus the predicate is the set.

The substrate measures bytes, not people. It verifies that signatures are valid, that hashes match, that chains are well-formed. It does not verify that pubkeys belong to distinct entities, that signers are independent, or that any social fact about who they are is true. Pubkeys are pseudonymous; personhood and organizational independence live outside the protocol by design.

This is the same property that makes witness chains work. The protocol stays at finite, verifiable claims. The auditor's trust calculus over signers stays where it belongs: in the auditor. Bitcoin does not measure miner decentralization; it measures hashpower. Decentralization is an empirical property, not a protocol guarantee. Same shape here. The chain is what the substrate gives you. The trust through it is yours.

## 8. Three axes of pinning

A proof bundle binds three independent CIDs: the contract it conforms to, the witness chain that endorses it, and the binary it asserts about. Each axis is its own content-addressed object. Each can be pinned (frozen to a specific value) or floated (track the latest acceptable value at verification time). Eight combinations of pin and float across three axes give eight distinct trust postures.

| Contract | Witness | Binary | Use case |
|----------|---------|--------|----------|
| pin | pin | pin | Frozen audit snapshot, total reproducibility |
| pin | pin | float | "These auditors against this spec for the current build" — CI gate |
| pin | float | pin | "Any chain proving this binary against this spec" — regulatory |
| pin | float | float | "Some binary, somehow audited, against this spec" |
| float | pin | pin | "These auditors verified this build, against whatever spec is current" |
| float | pin | float | "I trust these auditors, applied to anything" |
| float | float | pin | "Exactly this artifact, anyone can pick contract and chain" |
| float | float | float | Reference latest of everything; default |

The substrate does not pick for you. It gives you three axes and lets you decide which to freeze and which to float, per use case. The pins themselves are mementos; you can have many simultaneously, for many purposes. Different teams pin differently for the same artifact: a security team holds tight witness pins, a dev team holds tight binary pins, a compliance team holds all three.

`package.json` today is one-dimensional pinning that conflates the axes. `"react": "18.2.0"` says one binary; the contract is implicit ("trust the maintainer's semver intent"); the witness is none ("trust npm"). On the substrate the same line splits cleanly:

```json
{ "react": {
    "binaryCid":   "blake3-512:...",
    "contractCid": "blake3-512:...",
    "witnessCid":  "blake3-512:..." } }
```

Three CIDs, three independent decisions, three axes evaluable per upgrade.

## 9. Semver, made cryptographically meaningful

Semver is making a contract claim, but as an honor-system promise from the maintainer with no enforcement. The substrate makes semver verifiable.

A patch upgrade claims "no contract change". Provable: the new binary's `contractCid` equals the old binary's `contractCid`, or it is not a patch regardless of the version string. A minor upgrade claims "contract extended, old API still works". Provable: the new binary mints a fresh attestation that the old `contractCid` is still satisfied. A major upgrade claims "incompatibility, explicit break". Provable: the new binary's `contractCid` has no bridge back to the old one; the break is named, signed, dated, witnessed.

`"react": "^18.2.0"` stops being a string-match against version labels and becomes a typed query: "any binary whose contract has a conformance bridge to `contractCid_18.2`". Resolution becomes proof, not maintainer intent. A binary claimed as a patch but with a different `contractCid` is a structurally visible lie, not a typo.

Semver is elevated from social convention to substrate primitive. The maintainer's intent and the cryptographic reality become the same statement, or they do not, and when they do not, the substrate shows you exactly where.

## 10. Closure: subsetting is hashing

Anything that is part of an existing hashed object is already addressable by hashing the subset. Composition is free. The substrate does not need primitives for sets, subsets, walks, vectors, or rollups, because all of them are queries over the existing leaves.

- "These three contracts from the rust bundle" is `hash(JCS([c1, c2, c3]))`.
- "The first 100 attestations witnessing contract C" is `hash(JCS(walk(C).take(100)))`.
- "Every contract minted by signer S in 2026" is `hash(JCS(walk().filter(by_signer_and_date)))`.
- "All catalog entries excluding deprecated specs" is `hash(JCS(catalog.minus(deprecated)))`.

The most useful instance: **`hash(JCS(<sorted contract CIDs>))` is a stable pin for a contract set.** If the set of contracts does not change, the hash does not change, regardless of which kit they live in, what order they were minted, or what witnesses have accumulated around them. Pin one CID and you have pinned the entire contract universe at a moment. Any kit can compute it, any auditor can verify it, no protocol-level meta-bundle is required. The composition is the pin.

None of these require substrate changes. None need new memento types. None need permission. Anyone holding the underlying data can compute the same CID independently and the result agrees byte-for-byte if they agree on the filter predicate. Disagreements are visible: two auditors with different filters compute different CIDs over the same DAG, and the difference is auditable.

The architectural rule: if something is part of an existing hashed object, asking for its CID is a query, not a request to the substrate. The protocol provides the leaves. Tooling computes views. Auditors who want a stable pin sign their own attestation over their view. The substrate stays at three primitives:

1. **Sign.** Bind a content-addressed object to a signer.
2. **Hash.** Produce a content-addressed CID for any byte string.
3. **Reference.** Embed a CID in another object so signing the outer transitively names the inner.

Everything else is composition. Witness chains, witness sets, witness vectors, contract bundles, kit rollups, semver checks, audit trails, package-manager pinning, three-axis trust postures: all of them are functions over those three primitives. The protocol resists feature creep because every feature anyone proposes can be expressed as "compute X from existing leaves, sign your view of X."

The substrate stays small. The composition layer is unbounded.

## 11. The address is multi-dimensional

In a content-addressing system, the same content lives at many addresses at once. Each address is a projection of the content into one chosen dimension: hash over the declaration alone, hash over the declaration with a signer, hash over a sorted set of declarations, hash over a serialized bundle that includes minting state. The content does not move between these addresses; it occupies all of them simultaneously, in different projections.

The substrate's first guarantee is that **same content produces the same address** in the dimension you asked for. The closure property of §10 depends on this. Two parties holding the same content get the same CID without coordination, byte for byte, because the projection is deterministic.

The failure mode is not getting the wrong content. It is getting the same content at a different address than expected, because the address dimension you chose includes things that aren't content. Pin the bundle file's bytes as if they were a contract identity, and the address moves every time anyone re-mints, even though every byte of every contract is unchanged. Same content, different address, because the dimension included signer state and mint timestamps. The pin breaks not because the content moved, but because the dimension asked what "same" means and answered with bytes that drift.

The four vectors the substrate actually pins on are projections at four different dimensionalities.

- `contractCid` projects over content alone (one ContractDecl).
- `contractSetCid` projects over content alone (a sorted set of contract CIDs).
- `attestationCid` projects over content plus signer plus declaredAt plus signature.
- A bundle file's CID projects over content plus all embedded envelopes plus producer metadata plus disk-layout artifacts.

The first two are content-addressed: same content, same address, across machines, signers, time. The third is signer-and-time-addressed by design: an attestation is supposed to be a unique witnessing event, distinguishable from another witnessing event of the same content. The fourth is build-artifact-addressed: useful as a warm-cache key, brittle as a trust anchor, because the address moves under every honest re-mint.

Choosing dimensionality is therefore the substrate's primary act. The four specs that formalize this choice (`contract-cid-vs-attestation-cid`, `contract-set-extension`, `substrate-layers-envelope-header-body`, `version-chains-pinning`) are not feature additions; they are the substrate naming the dimensions it is willing to converge on. Anything outside these dimensions is composition under §10, legitimate and free, but not a trust anchor.

The operational test: ask what shifts the bytes for unchanged content. If signer state, mint timestamp, or producer metadata can shift the bytes while every contract is byte-identical, the dimension includes more than content and the address will drift even when the pin should hold. If only the content can shift the bytes, the dimension is content-only and the substrate will converge. The substrate is correct precisely to the extent that nothing in it confuses the two.

## What this means for you

If you accept this framing, the developer move is direct: stop asking "which chain do I publish to?" and start asking "what discipline am I asserting on the bodies I sign?"

`provekit witness` is the operational surface. You build a binary, hash it, mint a `.proof` bundle whose `binaryCid` pins the artifact and whose discharges carry whatever evidence your discipline requires, then sign and ship the pair. The verifier hashes what it received, looks up the bundle, walks the chain under its policy, emits a report. The report is itself a memento another verifier can take as evidence.

You do not need a chain to do this. You do not need a network to do this. You need a producer key, a body discipline, and a consumer who agrees to walk your chain under their policy. The four invariants do the rest.

The blockchain people are doing one thing in a space that contains many. Pick your discipline.
