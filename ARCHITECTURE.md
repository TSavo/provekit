# ProvekIt Architecture

A walk-through of the protocol's mechanics in roughly fifteen minutes. This document describes the v1.1.0 protocol catalog at CID `blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106`. Every spec referenced here is itself content-addressed; CIDs are quoted where authoritativeness matters.

## The four-layer model

ProvekIt has exactly four moving parts. Each has a fixed contract; the rest is implementation choice.

### Layer 1: kit (authoring surface)

A **kit** is the per-language bundle that emits canonical IR. The kit owns: an IR library (the host-language types and helpers a developer or lift adapter uses to express formulas), an AST canonicalizer (host-source AST to IR), a producer integration (one or more shape walkers that drive the canonicalizer), and a diagnostic translator (mapping verifier output back to host-language terms). Kits are content-addressed; two kits with byte-identical components share a CID. The kit standard lives at CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88`.

A kit's job ends at "I have IR." Whatever the host language is, whatever the source annotation library was, the IR is the same bytes.

### Layer 2: IR (the canonical formula)

The IR is a deterministic context-free language over a finite production set. The formal grammar lives at CID `blake3-512:6c0127e0d24946d7be75861db20507ccdcfdf968d3333f8aa34083e849d8238d73b3acfaa31880648995a024112182ed6b6002cd489548b4b18f5d4c3768dd96`. The canonicalization pipeline (CID `blake3-512:4d8c2940c53a59c678c8fb65e33dc2cb0ae8ae8a283b97b9c69fd678565653d15e6ee9dc3ffc6a32dc1ff035821b0c1a006f0455498d2ea91faef845d7b39830`) runs eight passes: de Bruijn renumbering, sort canonicalization, predicate canonicalization, implies removal, NNF, AC normalization, JCS serialization, and BLAKE3-512 hashing.

Two formulas that are alpha-equivalent under canonicalization produce the same bytes. The bytes hash to the same CID. The CID is the formula's content-addressed identity. Hash equality is formula equality. This single fact carries most of the protocol's weight.

The IR extension protocol (CID `blake3-512:cff64b923879548fd54efb63c5ea116ba184adadec126e956387f1ee9d0f7907edbdb1a05866d62442a6fb2142948654464e63830a061efdaed8af9403bb0c13`) defines how new theories enter the IR without invalidating old hashes.

### Layer 3: memento envelope (the signed unit)

A **memento** is a signed envelope wrapping IR content with provenance, role, and a CID derived from the wrapper's canonical bytes. The memento envelope grammar (CID `blake3-512:58bba3e1a9f6439eac5cb0c681faf65d38de9e6b8ad539854acda451ca67562a9d238eb95a5d7df2c0776657015fa026c51059dff61e1ba9aa2438b57425d6a5`) defines six roles in v1.1.0:

- **contract**: a function's pre, post, and inv as a signed unit.
- **implication**: a signed witness that one IR formula implies another.
- **bridge**: a binding from a host-language symbol to a contract CID.
- **catalog**: a named collection of CIDs (the protocol's own version is a catalog memento).
- **suite-result**: an aggregated test outcome.
- **producer-attestation**: a signed claim about which kit produced what.

Every envelope carries `bindingHash`, `propertyHash`, and `cid` as DERIVED fields. Validation recomputes them; drift fails closed. Signatures are Ed25519 over canonical envelope bytes per the signatures spec at CID `blake3-512:8b71229fcb7413f18a93a9b260012298311c1ce754850ee717780c181f1fda39a6600b2e5069e775cd7dd15e8c81e40b47bf7585aa0b23ab76c112c85116365c`.

### Layer 4: proof envelope (the published catalog)

A **`.proof` file** is a CBOR-encoded catalog of mementos, addressed by its own CID. The proof file format (CID `blake3-512:7bb4589af25c6c3992520494869bbbe4cfbcf7a77b91ebd61d6327e78699ef16cd5bc34afbe4cdf88a717c055c16536b5106bc4dca2d9d6b5cfcc1eede68e1b3`) specifies RFC 8949 §4.2.1 deterministic encoding (sort by bytewise CBOR-encoded form). The format is hand-rolled because off-the-shelf CBOR libraries do not enforce the sort rule that determinism requires.

A library publishes a `.proof` alongside its bytes. A consumer's verifier walks `<projectRoot>` and `<projectRoot>/node_modules/{*,@*/*}/` (or `target/release/lib*.rlib`-style paths in Rust), pools every memento it finds, and indexes by CID, by symbol, and by the (`post-hash`, `pre-hash`) pair.

## The handshake algorithm

The handshake (CID `blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925`) is the protocol's cost model. For every call site `g(f(x), ...)`, the verifier resolves `f`'s contract and `g`'s contract by walking the bridge for each symbol, then asks: does `f.post` imply `g.pre`?

Three tiers, in order:

**Tier 1: hash equality.** If `f.post` and `g.pre` canonicalize to the same bytes, their hashes are equal, and the call site is discharged for free. The verifier returns `DISCHARGED_BY_HASH`. This is the librarian: content-addressed lookup, no proof obligation, no solver invocation.

**Tier 2: cached implication.** The verifier looks up `(f.post-hash, g.pre-hash)` in the implication index. If a signed implication memento exists, the verifier checks the Ed25519 signature once and returns `DISCHARGED_BY_CACHE`. Every call site sharing this `(post, pre)` pair is discharged together.

**Tier 3: solver fallback.** Z3 is invoked exactly once per genuinely-novel `(post, pre)` pair. On `unsat`, the verifier mints a fresh implication memento (with the SMT-LIB script and the prover identity for replay), signs it, writes it to the verifier's policy-determined output (local, project `.proof`, or public registry), and returns `DISCHARGED_BY_SOLVER`. Every future verifier hits Tier 2.

If Tier 3 fails, the verifier falls back to per-call-site Z3 with the actual argument substituted into the consumer's `pre`. This is the residue: only genuinely consumer-specific pre-conditions reach this path.

The handshake's report includes a discharge breakdown:

```
total call sites:        N
discharged by hash:      M    (free; structural equality after canonicalization)
discharged by cache:     K    (Tier 2; cached implication memento)
discharged by solver:    L    (Tier 3; Z3 ran once, memento minted)
flagged per call site:   J    (residue; per-call-site Z3)
violations:              V    (Z3 returned sat; counterexample)
```

`M / N` is the **hash-discharge fraction**: the share of work the protocol amortized to zero. Higher is better; the metric is observable per run.

## The implication memento

The implication memento is the unit of cached reasoning. Its body carries:

- `antecedentCid`, `consequentCid`: the two contract CIDs whose post and pre are being related.
- `antecedentHash`, `consequentHash`: the canonical formula hashes (BLAKE3-512).
- `antecedentSlot`, `consequentSlot`: which slot each formula occupies (`post` or `pre`).
- `prover`, `proverRunMs`: which solver minted the witness, and how long it ran.
- `smtLibInput`, `proofWitness`: optional replay payloads.

The wrapper's signature, by an Ed25519 key registered to the producing prover, asserts non-repudiation: "I, this prover, ran on this input, and concluded the implication." Anyone can replay; replay disagreement is itself a memento (a `suite-result` recording the disagreement). The implication-memento body is fixed-arity; validation cost is `O(|m|)` and bounded.

The contract merge semantics spec (CID `blake3-512:aeb9e2c56603f56372c29cdcbbb11ec3ae6fada0b2004d9fb99955e21230b03a72d02fc7051eea09ed24b7271b758909011948b531b668bb5c20e8ab8a268bee`) defines what happens when two contract mementos disagree on the same symbol: a deterministic merge over canonical bytes, fail-closed on conflict.

## The lattice tractability theorem

The verification problem lives over a graph: vertices are populated CIDs, edges are signed implication mementos. The lattice tractability theorem (CID `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07`) proves four mechanical claims and sketches two more:

1. The populated address space at any AST depth `D` and symbol-count bound `S` is finite. The 2^512 hashspace is a property of the address space, not of the search space.
2. The population is bounded by a computable function of `(D, S)`; at small bounds, exhaustive enumeration is feasible.
3. Address computation is `O(|s|)` in the input formula's byte length; independent of the populated cardinality and the cryptographic security parameter.
4. Edge verification is `O(|m|)` per memento, bounded for fixed-size signature schemes.
5. (Sketch) Edge production reduces to the underlying decision procedure on `(ψ_u, ψ_v)`, not on the hash bytes.
6. Adversarial cost is confined to cryptographic preimage attack on BLAKE3-512, infeasible by assumption.

The honest-cost path is disjoint from the attack path. Honest verifiers operate on grammar parameters and proof-system decision complexity. The 2^512 cardinality governs only what an adversary cannot do.

The corollaries land the operational claims: verifier cost is decoupled from the cryptographic parameter; lattice density is calculable; cache invalidation is structurally absent; the lattice grows by independent local edge production with no global coordinator.

## Cross-language conformance

The IR is language-agnostic. A Rust kit, a TypeScript kit, and a Go kit all emit the same canonical bytes for the same canonical formula. A contract memento minted by the Rust kit and a contract memento minted by the TypeScript kit, expressing the same proposition, share a CID. The handshake at Tier 1 sees them as identical. This is the cross-language conformance property: a TypeScript consumer of a Rust library has the same Tier-1 discharge fraction as a Rust consumer would.

The semantic envelope (CID `blake3-512:6b14a0c4a36877ea77a609f5262257fb4c65940e8e5eefc03647746525d052216761b1707384bffec5fb3c021ac0e07e45d390efefa2c7f5c67c045d93352b56`) and the supply-chain spec (CID `blake3-512:b924576310bf2defcc2e346e01bdaff6dbdb2a33b2554178e3786e484cb4a4da136443f17675a6cd939fcbcff8afb27396222b5b6649a6eaa31672f5446b15d0`) define how kits attest to producing the bytes they emit, and how a consumer can verify that the bytes shipped by a publisher were in fact produced by the kit the publisher claims.

## Fail-closed posture

The chain validity spec (CID `blake3-512:dd905e8660d855c0c8140ceaafb0e189391234ff981422e714644b23642b24d7ca0253c76d09b46e58c5f8d8362cc8efca436baf2be927ab63a7bacf356e7673`) defines the protocol's failure modes. Every gate is fail-closed:

- A memento whose recomputed CID does not match the embedded CID is rejected.
- A memento whose signature does not verify is rejected.
- A `.proof` file with malformed CBOR is rejected.
- A handshake at Tier 3 that times out returns `REQUIRES_PER_CALLSITE`, never a false positive.
- A protocol catalog whose CID does not match the implementation's declared conformance is rejected.

There is no "best effort" mode. There is no "soft fail" mode. The verifier either has a discharge witness or it does not.

## Build-script integration (planned for v1.2)

The Rust build-script integration (`provekit-build`, in flight) lifts contract violations into compile-time errors. The build script invokes `cargo provekit-lift` and `provekit prove`; on a non-zero exit, the build fails. The result: contract violations join the compiler's existing error stream, alongside type errors, borrow-check errors, and lint failures. ProvekIt becomes a smarter type system extension, not a runtime probe.

This is what makes "constraint-driven development" name a real concept: the constraint is enforced at the same gate as the type system, not at a later stage.

## What this architecture does NOT include

ProvekIt does not author specifications. The kit emits IR; the lift adapter reads existing source-language annotations; the developer keeps writing `proptest!`, `#[contracts::ensures]`, `pydantic.BaseModel`, `zod.object`, or whatever idiom their codebase already uses. The protocol's job is to canonicalize, hash, sign, and check. The act of describing what should be true belongs to the libraries the codebase already trusts.

This is the lift-not-author posture. ProvekIt sits beneath every annotation library; it does not compete with any of them. The architecture's surface area is correspondingly small: four layers, one handshake, one tractability theorem, one canonical IR. The rest is implementation.

## Read further

- [README.md](README.md) for the install path.
- [PRODUCT.md](PRODUCT.md) for what ProvekIt replaces and complements.
- [THESIS.md](THESIS.md) for the deeper architectural claim.
- [docs/per-language-status.md](docs/per-language-status.md) for kit and adapter coverage.
- [protocol/specs/](protocol/specs/) for the canonical spec set, addressed by CID.
