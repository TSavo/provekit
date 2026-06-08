# Sugar: Lattice Tractability Theorem

**Date:** 2026-04-30
**Status:** Specification (formal). Companion to the IR formal grammar, the memento envelope grammar, and the handshake algorithm.
**Audience:** Referees, implementers, and operators reasoning about verification cost at scale.

## Abstract

We give a formal account of why Sugar's verification problem is tractable for honest participants despite operating over a cryptographic-hash address space of cardinality 2^512. The protocol's address space is content-derived: addresses arise only by hashing IR formulas the grammar admits. The IR grammar is a deterministic context-free language, parameterized in practice by AST depth D and symbol count S, with a finite (and at small bounds enumerable) population. The verification lattice is the directed graph whose vertices are the populated addresses and whose edges are signed implication mementos. We prove four mechanical claims: finite population at any (D, S) bound, linear-time addressing, constant-time edge verification, and adversarial-cost confinement to cryptographic preimage attack. We sketch two subtler claims: a parameterized population bound and the reduction of edge production to theorem-prover decision complexity. The consequence is that honest verification cost is a function of grammar parameters and proof-system decision complexity, not of the cryptographic security parameter.

## 1. Notation

Let the following symbols be fixed throughout.

- `Σ`: the byte alphabet, `{0, 1, ..., 255}`.
- `Σ*`: finite byte strings.
- `G`: the deterministic context-free grammar of the kit-emitted IR-JSON external form, defined in `2026-04-30-ir-formal-grammar.md`. Its productions are imported by reference and not reproduced.
- `L(G) ⊂ Σ*`: the language of `G`. Every `s ∈ L(G)` is a complete IR `Document` (an array of `Declaration`s).
- `C`: the canonicalization pipeline of `2026-04-30-canonicalization-grammar.md`. `C` consumes a parsed IR value, runs the eight passes (de Bruijn renumber, sort and predicate canonicalization, implies removal, NNF, AC normalization, JCS serialize, hash), and emits a canonical byte form whose JCS serialization is a `Σ*` string. Write `C(s)` for the canonical bytes of the IR value parsed from `s`.
- `L_C(G) := { C(s) : s ∈ L(G) }`: the language of canonical strings induced by `G` and `C`. Every distinct canonical-equivalence class of IR values has exactly one element in `L_C(G)`.
- `H : Σ* → {0, 1}^512`: the BLAKE3-512 hash, modeled as a random oracle. We assume the standard BLAKE3 properties: collision resistance, preimage resistance, second-preimage resistance, each at the BLAKE3-512 security level (256-bit classical, ≈170-bit post-quantum via Brassard-Høyer-Tapp).
- `Sign / Verify`: Ed25519 over canonical bytes, per `2026-04-30-signatures-and-non-repudiation.md`.
- A **memento envelope** is a `ClaimEnvelope` instance per `2026-04-30-memento-envelope-grammar.md`. Roles relevant here are `contract` (a function's pre / post / inv as a signed unit) and `implication` (a signed witness that one IR formula implies another).
- `D ∈ ℕ`: an AST depth bound. `S ∈ ℕ`: an AST symbol-count bound. `(D, S)` jointly bound the size of any IR value under consideration.

The kit-emit form (insertion-order keys) and the canonical form (JCS, sorted keys, post-canonicalizer passes) are distinct encodings of the same IR value at different layers. The grammar `G` describes the former; the language `L_C(G)` is the population of the latter. Hashes are computed over the canonical form. A consumer of this spec who conflates the two gets the address-space cardinality wrong; we are careful below.

## 2. Definitions

**Definition 1 (Bounded IR population).** For depth bound `D` and symbol-count bound `S`, let `L(G, D, S) ⊆ L(G)` be the subset of IR values whose canonical AST has depth at most `D` and at most `S` total internal nodes (counting quantifiers, connectives, atomic predicates, terms, and sort references). Equivalently, `L(G, D, S)` is the projection of `L(G)` onto AST trees fitting within the box `(D, S)`.

**Definition 2 (Populated hashspace at bound `(D, S)`).** Let `R(D, S) := { H(C(s)) : s ∈ L(G, D, S) }`. Each element of `R(D, S)` is a 512-bit hash of a canonical-form IR value within the bound. `R(D, S) ⊆ {0,1}^512`.

**Definition 3 (Verification lattice at bound `(D, S)`).** Let `Lat(D, S) := (V, E)` where:

- `V := R(D, S)` (populated hashes are vertices),
- `E := { (u, v) : ∃ memento m of role "implication", with body's antecedentHash = u, consequentHash = v, signature valid under an acceptable prover key }`.

Edges are directed and signed. An edge asserts that the formula whose canonical hash is `u` universally implies the formula whose canonical hash is `v`, with a publishable Z3 (or compatible solver) witness as evidence.

**Definition 4 (Discharge at a call site).** A call site with publisher post-formula hash `u` and consumer pre-formula hash `v` is **discharged at bound `(D, S)`** iff `u = v` (Tier 1: hash equality) or `(u, v) ∈ E` (Tier 2: cached implication). When neither holds, the verifier falls back to Tier 3 per `2026-04-30-handshake-algorithm.md`.

This is the lattice over which the handshake algorithm operates. The remainder of the spec is concerned with its tractability properties.

## 3. Claims

We state six numbered claims. Claims 1, 3, 4, and 6 admit mechanical proofs; we give them. Claims 2 and 5 admit only sketches; we mark them so.

### Claim 1 (Finite population)

For every fixed `(D, S)`, `|L(G, D, S)| < ∞`, and consequently `|R(D, S)| < ∞` and `|V(Lat(D, S))| < ∞`.

**Proof.** `G` has a finite set of nonterminals, a finite set of productions, and a finite terminal alphabet. Every IR value is a finite tree whose nodes draw from this finite production set. The set of trees of depth `≤ D` with `≤ S` internal nodes drawn from a finite production alphabet is a finite set: there are at most `(production-count)^S` distinct unlabeled trees with `S` nodes, and at most a constant factor more once leaves (variable names, constants, sort tags) are accounted for, all of which range over countable but bounded-at-`(D, S)` sets. The map `s ↦ C(s) ↦ H(C(s))` is a function, so `|R(D, S)| ≤ |L(G, D, S)| < ∞`. Hence `|V(Lat(D, S))| ≤ |R(D, S)| < ∞`. ∎

### Claim 2 (Parameterized population bound, sketch)

`|L(G, D, S)|` is bounded by a computable function `B(D, S, |G|)` of the depth bound, the symbol bound, and the grammar size. The bound is at worst exponential in `S` and grows astronomically with `D`. We do **not** claim polynomial growth. We claim *enumerability at small bounds*.

**Sketch.** Count distinct IR ASTs by structural induction on depth. At depth 0, the population consists of leaf terms (variable references, constants over each primitive sort, nullary atomic formulas). Let `b` be the maximum branching factor of any production (the maximum arity of any node kind: connectives are bounded by the spec's arity rules; quantifiers have a single body child; atomic and ctor nodes' arities are bounded by `S`). Then the number of trees of exact depth `d` and at most `S` symbols is bounded above by `O(b^S)` per the standard catalog argument for trees over a finite alphabet, with a multiplicative factor accounting for leaf labels (variable names drawn from a bound determined by `S`, constants drawn from the canonicalized literal pools). Summing over `d ∈ {0, 1, ..., D}` yields a bound exponential in `S` and depth-bounded by `D`. The bound is tight enough to be operationally useful at small `(D, S)`: at `(D, S) = (3, 10)` the population is small enough to enumerate exhaustively on commodity hardware. At `(D, S) = (10, 50)`, exhaustive enumeration is no longer feasible, but the lattice can be populated demand-driven by the kits and the implication server; coverage is then an empirical density question rather than an exhaustive-enumeration question.

The honest content of this claim is that the population is **finite and computable at any bound**, not that it is polynomial. The cost-tractability of verification does not rest on a polynomial bound on `|R|`; it rests on Claims 3 and 4, which decouple verifier cost from `|R|` entirely. ∎ (sketch)

### Claim 3 (Address computation is linear in input size)

For any `s ∈ L(G, D, S)`, computing `H(C(s))` runs in time `O(|s|)` where `|s|` is the byte length of the kit-emit JSON. The runtime is independent of `|R(D, S)|` and of `|V(Lat(D, S))|`.

**Proof.** The pipeline `s ↦ parse(s) ↦ C(parse(s)) ↦ JCS(C(parse(s))) ↦ H(JCS(...))` decomposes into four stages.

1. **Parse.** The reference parser specified in `2026-04-30-ir-formal-grammar.md` is a recursive-descent parser over a deterministic context-free grammar with a `kind`-discriminator at every node. Each input byte is consumed by exactly one production rule. Running time is `O(|s|)`.

2. **Canonicalize.** Per `2026-04-30-canonicalization-grammar.md`, passes 1 through 6 (de Bruijn renumber, sort canonicalization, predicate canonicalization, implies removal, NNF, AC normalization) each visit each AST node a bounded number of times. With `n` AST nodes and `|s| = Θ(n)` bytes, each pass runs in `O(n)`, so canonicalization runs in `O(n) = O(|s|)`.

3. **JCS serialize.** RFC 8785 serialization sorts object keys lexicographically and emits in a single tree walk. Sorting at each object node is `O(k log k)` for `k` keys; summed over the whole tree, the cost is `O(n log K)` where `K` is the maximum keys at any object, bounded by a constant from the grammar (every node kind's key set has constant size). Hence `O(n) = O(|s|)`.

4. **Hash.** BLAKE3-512 is computable in linear time over input bytes: the BLAKE3 specification (`https://github.com/BLAKE3-team/BLAKE3-specs`) specifies a Merkle-tree-of-blocks construction whose total work is `Θ(L)` in the byte length `L` of the canonical input. Hence `O(L) = O(|s|)`.

The composition runs in `O(|s|) + O(|s|) + O(|s|) + O(|s|) = O(|s|)`. None of these stages references `R(D, S)` or `V(Lat(D, S))`. ∎

### Claim 4 (Edge verification is constant-time, plus signature)

Given a candidate implication memento `m` and the local indices specified in `2026-04-30-handshake-algorithm.md`, the verifier decides whether `m` represents a valid edge of `Lat(D, S)` in time `O(|m|)`, which for fixed-size signature schemes (Ed25519) and bounded body fields is `O(1)` per edge. The runtime is independent of `|V|`, `|E|`, `|R(D, S)|`, and the cryptographic security parameter.

**Proof.** Edge validation comprises four subchecks.

1. **Wrapper shape.** CDDL acceptance of the envelope's role-specific structure (per `2026-04-30-memento-envelope-grammar.md` §"Role: ImplicationMemento"). CDDL validation is linear in the envelope size; envelope size is bounded for implication mementos (the body is a fixed-arity record of two hashes, two CIDs, two slot tags, a prover ID, a runtime, and optional SMT-LIB and proof-witness blobs).

2. **Hash equality.** The validator recomputes `bindingHash`, `propertyHash`, and `cid` per the DERIVED rules and compares to the bytes in `m`. Each comparison is a constant-time string equality on a 512-bit-tagged hash.

3. **Referent lookup.** `inputCids[0]` and `inputCids[1]` are looked up in `contracts_by_cid`, which is a hash map. Each lookup is `O(1)` expected.

4. **Signature verification.** Ed25519 verify on the canonical envelope bytes. The Ed25519 specification (RFC 8032) yields verification in time independent of message length up to a linear scan; for envelope bytes of bounded size, the cost is `O(1)`.

No step iterates over `V`, `E`, or `R(D, S)`. The total is `O(|m|)`, which is `O(1)` for envelopes whose body fields are bounded. ∎

### Claim 5 (Edge production is decision-complexity-bounded, sketch)

Producing a witness for a candidate edge `(u, v)` reduces to deciding `forall x. ψ_u(x) → ψ_v(x)` in the underlying logic interpretation, where `ψ_u` and `ψ_v` are the IR formulas whose canonical hashes are `u` and `v`. The cost is the cost of the proof procedure, which depends on the fragment of first-order logic the formulas inhabit, not on `|R(D, S)|` and not on the cryptographic security parameter.

**Sketch.** The IR's atomic predicates and ctors range over a finite set of theories: linear integer arithmetic, linear real arithmetic, equality with uninterpreted functions (EUF), bitvectors, finite-domain set membership, and the kit-defined extensions specified in `2026-04-30-ir-extension-protocol.md`. For decidable fragments (LIA, LRA via quantifier elimination; EUF; bitvectors with bounded width), the decision procedure is at worst exponential in formula size; for the practical IR sizes encountered at `(D, S) = (10, 50)`, Z3's portfolio dispatches in milliseconds to seconds. For semi-decidable fragments (full first-order logic with quantifier alternation), the procedure is bounded by the prover's heuristic budget; the protocol specifies fail-closed semantics: a timeout returns "REQUIRES_PER_CALLSITE" rather than a false positive. In neither case does the cost depend on `|R(D, S)|`: the prover operates on the formulas' logical interpretation, not on the hash bytes. The cryptographic security parameter governs only the unforgeability of the resulting memento, not the proof effort. The 2^512 hash space never enters the search bound. ∎ (sketch)

### Claim 6 (Adversarial cost is confined to cryptographic attack)

The only operation requiring `2^Θ(security parameter)` work is preimage, second-preimage, or collision attack on `H = BLAKE3-512`, which by assumption is infeasible: 256-bit classical security and approximately 170-bit post-quantum security via Brassard-Høyer-Tapp. Honest participants never face this cost.

**Proof.** Honest production of an edge `(u, v)` proceeds as follows: choose two IR values `s_u`, `s_v ∈ L(G, D, S)`; compute `u = H(C(s_u))`, `v = H(C(s_v))` (Claim 3, linear); ask the prover whether `s_u → s_v` is valid (Claim 5, prover-bounded); if so, mint and sign the memento (Claim 4-shape, signature-bounded). At no step does the participant search for hash preimages or collisions. To **forge** an edge, an adversary would need to produce a memento `m` whose body asserts `(u, v)` and whose signature is valid, where the asserted hashes have no actual canonical-form preimages in `L(G, D, S)`. By the random-oracle assumption on `H`, finding any 512-bit string that lies in `R(D, S)` without first knowing the canonical preimage is infeasible. By the Ed25519 unforgeability assumption, fabricating a valid signature without the producer key is infeasible. The two assumptions together pin adversarial cost at `2^256` classical (preimage attack on BLAKE3-512) or `2^128` for collision; both are out of scope for any practical adversary. The honest cost path is disjoint from this attack path. ∎

## 4. Corollaries

### Corollary 1 (Verification cost is decoupled from the cryptographic parameter)

By Claim 3 and Claim 4, the verifier's per-call-site cost in Tier 1 (hash equality) and Tier 2 (cached edge) is a function of the canonical formula's byte length, the bounded signature scheme, and the constant-factor index lookups. The cryptographic security parameter (the `512` in `H : Σ* → {0,1}^512`) appears only in the byte width of the hash strings, contributing a constant factor. It never appears in the asymptotics. An honest verifier's cost is dominated by `|s|` for input parsing and proof-script construction, never by `|R|` or `2^512`.

### Corollary 2 (Lattice density is calculable)

Coverage is a measurable property of the lattice. At bound `(D, S)`, the denominator `|L_C(G, D, S)|` is finite and computable (Claim 1, with concrete enumeration at small bounds per Claim 2). The numerator `|E(Lat(D, S))|` is the count of published implication mementos whose `antecedentHash` and `consequentHash` both lie in `R(D, S)`; this count is observable by any party walking the published memento store. Hence the question "what fraction of provable implications at bound `(D, S)` have been minted?" has a numerical answer, modulo two real obstructions: (i) most pairs `(u, v)` are not provable implications, so the relevant denominator is `|{(u, v) : ψ_u → ψ_v is logically valid}|` rather than `|V|^2`; (ii) ground-truth on (i) requires running the prover on every pair, which is itself the work the lattice amortizes. Operationally, the implication server reports observed `|E|` and the verifier reports the discharge breakdown specified in `2026-04-30-handshake-algorithm.md` §"Reporting"; coverage is the ratio of cache hits to discharge attempts. This makes the implication server's value to the ecosystem a measurable empirical quantity, not a hand-waved one.

### Corollary 3 (Cache invalidation is structurally absent)

Hashes are deterministic functions of canonical bytes. If an IR formula's bytes change, its canonical bytes change (`C` is a function), and its hash changes (`H` is a function). Old implication mementos remain cryptographically valid with respect to their stated `(antecedentHash, consequentHash)`; they simply become unreachable from any contract that has been re-canonicalized. There is no "stale cache entry" scenario: an entry either matches by hash or it doesn't, and old entries describing now-orphaned hashes neither falsify nor poison the lattice. This corollary is the same primitive observed at the protocol layer in `2026-04-30-handshake-algorithm.md` §"Three-tier discharge"; we cross-reference rather than re-derive.

### Corollary 4 (The lattice is incrementally computable)

Each candidate edge `(u, v)` is decided by a single solver invocation operating on `(ψ_u, ψ_v)` alone. The solver does not consult `V`, `E`, `R(D, S)`, or the populated lattice in any form. Mement minting (Claim 4-shape) is a local operation. Verification of a minted memento is a local operation (Claim 4). Hence `Lat(D, S)` grows by union of independently-produced edges; no global coordination is required, no consensus protocol, no central authority. This is the property that makes the implication server an indexer rather than a producer: the server crawls published mementos, indexes them by `(antecedentHash, consequentHash)`, and serves them; it does not mediate proof production.

## 5. Practical bounds

### Order-of-magnitude estimate at `(D=10, S=50)`

For a kit-emit grammar with branching factor approximately `b ≈ 8` (the rough fan-out of operands at a connective node), trees of `S = 50` internal nodes admit `O(b^S) = O(8^50) ≈ 10^45` distinct shapes. Most of these shapes do not correspond to well-typed IR values once sort and predicate-arity constraints are applied; the "well-typed at `(D, S)`" subset is some many orders of magnitude smaller, but still astronomical. **The lattice at full `(D, S)` is not enumerable.**

This is fine. The lattice does not need exhaustive enumeration to be useful. The lattice needs density along the **observed** call sites of the ecosystem. A typical TypeScript codebase exercises a small slice of `R(D, S)`; a typical Rust crate, another slice; a typical kit's published catalog, a third. The intersection of slices is where Tier 1 (hash equality) discharges. The union of slices, plus minted implications, is where Tier 2 caching pays off. The implication server's job is to converge on dense coverage of the **observed slice**, not the combinatorial population.

### Demo target: `(D=3, S=10)`

For Stage 4 of the implementation roadmap, a small-bound demo at `(D, S) = (3, 10)` is enumerable by brute force on commodity hardware. With `b = 8` and `S = 10`, the upper bound is `8^10 ≈ 10^9`; the well-typed subset is on the order of `10^6` to `10^7`. Enumeration plus a single Z3 invocation per pair `(u, v)` populates a complete `Lat(3, 10)` in compute on the order of `10^14` solver invocations, which is not tractable as a single batch but is tractable as a long-running implication-server backfill. Whether full coverage at `(3, 10)` is an interesting demo target or a distraction is left to the implementation team; the math says it is feasible if pursued.

## 6. Connection to the architecture

The handshake algorithm (`2026-04-30-handshake-algorithm.md`) is the operational form of the lattice:

- **Tier 1 (hash equality, free)** is Claim 4 with zero work: equal hashes need no edge.
- **Tier 2 (cached implication, `O(1)`)** is Claim 4 in full: an edge of `Lat(D, S)` discharges the call site.
- **Tier 3 (Z3 fallback, prover-bounded)** is Claim 5: edge production runs the underlying decision procedure once per `(post, pre)` pair, then mints the result for everyone else.

The implication memento envelope (`2026-04-30-memento-envelope-grammar.md` §"Role: ImplicationMemento") is the operational form of an edge: a CDDL-validated, hash-derived, Ed25519-signed record of `(u, v) ∈ E`. The envelope's wrapper enforces non-repudiation; the body's `prover`, `proverRunMs`, and optional `proofWitness` enable replay; the post-CDDL referent and DERIVED constraints anchor the edge to its endpoint contracts.

The protocol catalog (`2026-04-30-protocol-versioning.md`, `2026-04-30-protocol-catalog.json`) anchors this entire spec set. The catalog is itself a memento, and its CID is the protocol version. The recursive payoff: this theorem applies to the protocol's own self-description. The address space the theorem lives in includes the address of the theorem.

## 7. Open questions and future work

These are listed as *honest open work*, not as derivable corollaries.

### 7.1 Tight population bounds for the canonical IR

Claim 2 gives a worst-case bound exponential in `S`. The true population at `(D, S) = (10, 50)` is many orders of magnitude smaller once sort-typing, predicate-arity, and canonicalization equivalence-class collapse are accounted for. A tight bound would need to count canonical-equivalence classes after `C` has run, not raw parse trees. This is a finite combinatorial counting problem; it is open.

### 7.2 Empirical density at small bounds

Stage 4 demo target. Build `Lat(3, 10)` by enumerating `L_C(G, 3, 10)`, running Z3 on every pair, and reporting the resulting density. The output is a single number per bound and is the empirical answer to Corollary 2's question.

### 7.3 Composition of edges (genuinely open)

Modus ponens at the proof-theoretic layer says: a witness for `(u → v)` and a witness for `(v → w)` should yield `(u → w)`. At the **memento layer**, this is not currently expressible. The `implication-evidence` body in `2026-04-30-memento-envelope-grammar.md` carries a single prover identity, a single SMT-LIB script, and a single runtime; it does not carry a *composition certificate* identifying two ancestor mementos as the building blocks. A verifier presented with two witnesses for `(u, v)` and `(v, w)` and asked to discharge `(u, w)` has two options: (a) trust the chain by verifying both ancestor signatures and the fact of slot-equality at `v`, accepting `(u, w)` without a fresh prover invocation; (b) re-prove `(u, w)` from scratch via Z3.

Option (a) is logically valid but requires either:

- a new memento role, **composed-implication**, whose body names two ancestor implication CIDs and whose validator checks slot equality at the join point, or
- an extension of the existing `implication-evidence` body with optional `viaImplications: [cid, cid]` fields and a corresponding DERIVED rule that the consequent of the first equals the antecedent of the second.

Either is a protocol-level addition. Until one is specified, composition is a Tier-3 operation: the verifier re-invokes Z3, possibly cheaper than the underlying primitives because the SMT scripts compose, but not a free chain.

We mark this as future work and **do not derive it as a corollary**. The current spec does not support free chaining.

### 7.4 Cross-bound coherence

If a contract exists at `(D, S) = (5, 20)` and the same contract is consumed at a code site where `(D, S) = (10, 50)` is the relevant bound, the lattice operations span bound boundaries naturally (the same hashes, the same edges). The formal claim is straightforward; we have not yet written it.

### 7.5 Quantitative comparison to the cryptographic parameter

A clean diagram comparing the asymptotic curves of (a) honest verifier cost in `n = |s|`, (b) prover cost in `(D, S, theory-fragment)`, and (c) adversarial cost in `2^k` for `k ∈ {256, 170}` would make the decoupling claim of Corollary 1 visually load-bearing. Future expository work, not protocol work.

## 8. Conclusion

The verification lattice is a finite content-addressed graph at any practical complexity bound. Honest verification cost is a function of grammar parameters and decision-procedure complexity; it is independent of the cryptographic security parameter and independent of the populated cardinality of the address space. Adversarial cost is confined to cryptographic attack on the underlying primitives, which is by assumption infeasible. The lattice grows incrementally by independent local edge production. Coverage is empirically measurable via the implication server's index. Composition of edges is the subject of future protocol work.

The 2^512 cardinality of the BLAKE3-512 address space is a property of the address space, not of the search space. The search space is `L_C(G, D, S)`, which is finite, computable, and at small bounds enumerable. Sugar's verification problem lives in the search space. The cryptographic parameter governs only what an adversary cannot do.
