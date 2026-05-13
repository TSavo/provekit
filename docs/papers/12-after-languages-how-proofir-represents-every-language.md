# After Languages: How ProofIR Represents Every Language

> **Status.** Sustained argument. Contains nine lemmas with constructive proofs or proof sketches. Engages counterarguments. Written to be cite-able.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md), [07 After Verification](07-after-verification-bug-classes-as-missing-edges.md), [08 After Types](08-after-types-stop-logging-trust-the-invariant-solver.md), [10 After Protocol Specs](10-after-protocol-specs-how-protocols-actually-evolve.md), [11 After Commits](11-after-commits-proof-carrying-change.md).
>
> **Companion spec.** [Algorithm Memento Protocol (AMP) v0.1.0](../../protocol/specs/2026-05-09-algorithm-memento-protocol.md).
>
> **Premise the earlier papers established.** A content-addressed federated substrate of canonical predicates, with signed implication edges, witnesses from a portfolio of solvers, droppers that close the loop with lifters, and a federation that asymptotically reduces verification cost to cache lookup. After Verification argued that this substrate makes leaf-discharge bug classes structurally impossible. After Types argued that invariants replace logging as the primary epistemic instrument.
>
> **What this paper argues.** That the substrate's first axiom, *Supra omnia, rectum* (above all, correctness), has been load-bearing for substrate output but has not yet been applied to the substrate's own production mechanism. The lifters that emit substrate output have themselves been per-language reimplementations: one Python port, one Rust port, one C port, one Java port, and one Zig port of the same algorithm, no canonical reference, no content-addressed identity for the algorithm itself, no mechanically-detectable drift. We close the gap. Algorithms become content-addressed mementos; language-specific lifters become verifiable refinement-claims; the substrate hosts its own production. We prove federation, drift detection, soundness preservation, and Cousot equivalence as nine lemmas. The substrate finally applies its first axiom to itself.

## §0: The claim

A substrate that produces signed correctness-receipts for user code while making un-attested claims about its own production mechanism is structurally inconsistent. The first axiom *Supra omnia, rectum* binds the substrate's outputs; it must also bind the substrate's tooling.

Today, the lifter family is the largest violation of that axiom. The "if/else two-armed weakest-precondition narrowing" algorithm is implemented independently in `provekit-walk` (Rust), `provekit-walk-c` (C/libclang), `provekit-walk-py` (Python AST), `provekit-walk-java` (JavaParser), and `provekit-walk-zig` (Zig AST). Five copies. No canonical reference. No content-addressed identity for the algorithm itself. Drift is the default; agreement is unverified. The substrate produces canonical contracts via uncanonical machinery.

This paper argues that the right shape is:

1. Each algorithm gets a content-addressed memento: its identity is its CID.
2. Each language's lifter source becomes a *binding-claim* memento that asserts correspondence with an algorithm CID.
3. Discharge of the binding-claim's refinement obligation produces a signed receipt.
4. Federation across language ports happens by the algorithm CID, not by source-code sharing.

We prove that this discipline (a) makes drift between language ports mechanically detectable, (b) preserves soundness across all bindings of the same algorithm, (c) embeds Cousot's lattice of abstractions as a content-addressed catalog, (d) extends naturally to the algebraic-effects unification of interaction primitives, and (e) makes the substrate self-hosting at the production layer.

## §1: The substrate's first axiom and its violations

The first axiom is short. *Supra omnia, rectum.* Above all, correctness. Every ProvekIt decision flows from this; shipping incorrect substrate is self-defeating.

The substrate has applied this axiom rigorously to its outputs. Lifted contracts are canonical IR. Composition is byte-deterministic. Signed mementos carry their own attestation. Witness pluralism gives every consumer a choice of which solver's verdict to accept. The substrate's *output* layer has been disciplined.

The substrate's *input* layer has not. The lifters are ordinary code. Their algorithms are not content-addressed. Two implementations of the same algorithm in different languages cannot be mechanically asserted to be the same algorithm; they can only be informally claimed to be the same. When five language ports each implement "weakest-precondition propagation through let-bindings," the substrate has five algorithms, not one with five bindings.

This is a substrate that produces correctness-receipts about user code while making uncontrolled claims about its own production mechanism. It is structurally inconsistent. The first axiom binds the substrate to itself; the substrate has been violating that.

This paper closes the violation.

## §2: Background and prior papers

Earlier After-X papers established:

- The substrate as a thin Heyting category of canonical predicates with content-addressed signed implication edges (paper 07, §3).
- Lifter and dropper as the substrate's bidirectional bridge to native code (paper 07, §2).
- Reputation displaced from the substrate to the policy layer (paper 06).
- Invariants replacing logging as the load-bearing epistemic instrument (paper 08).
- Lossy boundary compression as the principled handling of representation gaps (paper 09).
- Protocol Evolution Protocol (PEP) handling spec changes (paper 10).
- Proof-Carrying Change as the commit-level discipline (paper 11).

This paper extends the federation principle one level inward. The substrate's *production mechanism*: the lifters that emit canonical predicates: is itself reified as substrate participants under PEP-governed evolution.

## §3: The mechanism (summary)

The full normative specification is in [AMP v0.1.0](../../protocol/specs/2026-05-09-algorithm-memento-protocol.md). The summary:

- An **algorithm memento** is a `FunctionContractMemento` (per CCP) with conventions: the formals describe the abstract input shape (an `ASTPattern`), the post describes the canonical output formula. The memento's CID is the algorithm's identity.
- A **binding-claim memento** is a `FunctionContractMemento` whose formals describe a language-specific AST shape, whose `input_cids` reference the algorithm being bound, and whose post is a refinement obligation: "for every input I accept, my output equals the algorithm's output on the projected input."
- A **projection memento** per language defines the canonical projection from language-specific AST to abstract `ASTPattern`.
- Discharge of the binding-claim's refinement obligation produces a signed receipt verifying correspondence.

Federation is by algorithm CID. Two language ports binding to the same CID *are* the same algorithm by the substrate's definition.

## §4: Nine lemmas

We state and prove the load-bearing properties of this discipline. Notation: `A` ranges over algorithm mementos; `B` ranges over binding-claim mementos; `P_L` is the projection memento for language `L`; `cid(X)` is the BLAKE3-512 of the JCS encoding of X; `discharge(B)` denotes the prove-portfolio's verdict on B's refinement obligation.

### Lemma 1 (Federation by CID)

> If two binding-claim mementos `B₁` and `B₂` both refine the same algorithm `A` (i.e. `cid(A) ∈ B₁.input_cids ∩ B₂.input_cids`) and both have `discharge(Bᵢ) = UNSAT` (i.e. their refinement obligations are valid), then for any inputs `i₁ ∈ Lang(B₁)` and `i₂ ∈ Lang(B₂)` such that `P_{L₁}(i₁) = P_{L₂}(i₂)`, we have `B₁(i₁) = B₂(i₂)`.

*Proof.* By AMP §2's refinement obligation, `B₁(i₁) = A(P_{L₁}(i₁))` and `B₂(i₂) = A(P_{L₂}(i₂))`. By hypothesis `P_{L₁}(i₁) = P_{L₂}(i₂)`, so `A(P_{L₁}(i₁)) = A(P_{L₂}(i₂))` by determinism of A. Therefore `B₁(i₁) = B₂(i₂)`. □

### Lemma 2 (Soundness Preservation)

> If algorithm `A` is sound (i.e. `A.post` holds whenever `A.pre` holds, under `A.effects`) and binding `B` refines `A` with `discharge(B) = UNSAT`, then `B` is sound.

*Proof.* By AMP §1.4's refinement clauses (1)-(3): every input B accepts is in A's pre-image after projection; B's output equals A's; B's effects are a subset of A's. Soundness composes through these inclusions. □

### Lemma 3 (Drift Detection)

> If `B` and `B'` both claim to bind algorithm `A`, and there exists an input `i` in their common language with `P_L(i)` well-defined and `B(i) ≠ B'(i)`, then at least one of `discharge(B)` and `discharge(B')` is `SAT` (refuted by counterexample).

*Proof.* By Lemma 1's contrapositive: if both discharges were UNSAT, both would equal `A(P_L(i))`, hence equal each other. Since they don't, at least one fails its refinement obligation. The portfolio surfaces the counterexample. Drift is detected and the violating binding is identified. □

This is the load-bearing payoff. Five language ports of one algorithm, when each is attested, are mechanically asserted to behave identically on corresponding inputs. Drift is not a worry the substrate must mitigate; it is a condition the substrate mechanically detects and reports.

### Lemma 4 (Algorithm Identity is Syntactic, not Behavioral)

> Two algorithm mementos `A` and `A'` are equal iff `cid(A) = cid(A')`. Behavioral equivalence (`∀i. A(i) = A'(i)`) does *not* imply CID equality; two syntactically-distinct specifications of the same behavior get distinct CIDs.

*Proof.* JCS canonicalization fixes the bytes of A and A' modulo whitespace and key ordering; behaviorally-equivalent A and A' with different formals or different post structures hash to different CIDs by collision-resistance of BLAKE3. □

The catalog therefore federates by *syntactic identity*, not behavioral equivalence. This is the right choice: behavioral equivalence is undecidable in general; syntactic CID equality is mechanical. When two syntactically-distinct algorithms happen to be behaviorally equivalent, an explicit `refines` link in one of them establishes the relation.

### Lemma 5 (Compositional Lifters)

> Let lifter `L` be the composition of bindings `B₁, B₂, ..., Bₙ`, each binding a distinct algorithm `A₁, ..., Aₙ`, applied to disjoint AST patterns. Then `L` is correct iff each `Bᵢ` is correct.

*Proof.* Disjoint AST patterns means each input matches at most one `Bᵢ`. The lifter's output is therefore the union of each binding's output on its matching subset. By Lemma 2, each `Bᵢ` is sound iff its refinement obligation discharges. The lifter's correctness reduces to the conjunction of the discharges. □

Substrate engineering implication: a lifter's correctness becomes a property of its constituent binding-claims, not of its source code as an undifferentiated mass.

### Lemma 6 (Language Closure)

> For any computable language `L` with a recursive AST representation, there exists a projection memento `P_L` such that the projected `ASTPattern` set covers the subset of `L`'s syntactic shapes recognizable by the algorithms in the registered catalog.

*Proof sketch.* `P_L` is a computable function from L's AST to the abstract `ASTPattern` type. For any algorithm `A` in the catalog, `A.pre` is a recognizer over `ASTPattern`. The pre-image `P_L⁻¹(A.pre)` is the subset of `L`-AST that `A` applies to, by composition of computable functions. Coverage is per-algorithm; the union over the catalog gives the lifter's recognized fraction of `L`. □

The catalog's coverage of any language is determined by which algorithms have been registered, not by per-language plumbing. Adding a new algorithm extends every language port's coverage simultaneously; adding a new language requires only a projection memento.

### Lemma 7 (Cousot Equivalence)

> Each algorithm memento `A` corresponds to an abstract domain pair `(γ, α)` in Cousot's sense, where `γ : IrFormula → ConcreteSemantics` is the concretization of A's output and `α : ConcreteSemantics → IrFormula` is the abstraction implicit in `A.pre`'s recognizer. The algorithm catalog as a whole is the reduced product of all registered abstract domains.

*Proof sketch.* `A.pre` partitions the concrete input space into "matched" and "unmatched"; the matched subset is the abstract domain's denotation. `A.post` gives the abstract value (an IrFormula) for each matched input. The pair `(α, γ)` is a Galois connection over the partial order on IrFormula (entailment). The catalog's joint inference over an input is the meet (in IrFormula's lattice) of every matching algorithm's output: this is precisely Cousot's reduced product. □

Implication: the substrate's algorithm catalog *is* Cousot's lattice of abstractions, made content-addressed. Cousot 1977 named the structure; the substrate gives it a federation mechanism. After Verification (paper 07) anticipated this with the Heyting category framing; this paper makes the lattice explicit.

### Lemma 8 (Algebraic Effects Embedding)

> Every concrete language primitive that interacts with context (channels, mutexes, generators, exceptions, async/await, allocation, lock acquisition, atomic operations) can be represented as an algorithm memento whose canonical output cites a content-addressed effect-signature memento. Cross-language federation of effects emerges from Lemma 1 applied to the effect-signature CID.

*Proof sketch.* By the algebraic-effects design (companion doc), every such primitive decomposes to an effect operation paired with a handler under a canonical effect signature. Each effect signature is a content-addressed memento. An algorithm memento for "lift Python `yield` to its effect signature" is one such; an algorithm memento for "lift Go `ch <- x` to the same effect signature" is another. Both bind to the same effect-signature CID. By Lemma 1, contracts emitted by these two algorithms federate at the effect-signature CID join key. □

The unification: the seemingly-vast list of language-specific interaction primitives (generators, channels, mutexes, semaphores, throws, awaits, allocations, locks) collapses into one structural primitive plus a content-addressed catalog of effect signatures. This paper's mechanism (algorithm memento + binding-claim) extends to host that catalog without new structure.

### Lemma 9 (Substrate Self-Hosting)

> The substrate's lifter family is itself a substrate participant. Each lifter binary's source code is content-addressable (`body_cid` of its binding-claim memento). The lifter's correctness is a substrate-verifiable claim. The substrate's first axiom *Supra omnia, rectum* applies to its own production mechanism by the same mechanism it applies to user code.

*Proof.* The binding-claim memento's `body_cid` field references the lifter's source code bytes (BLAKE3-512). The refinement obligation in `B.post` is itself an IrFormula. The discharge of that obligation is mechanical via the prove portfolio: the same portfolio that discharges any user-code contract. The lifter's correctness receipt is signed and stored in the substrate alongside any other receipt. □

This closes the inconsistency stated in §0. The substrate now applies its first axiom to itself, by the same mechanisms it applies to everything else, via the same content-addressed federation, with the same composability properties.

## §5: Counterarguments

We address the strongest objections.

### Objection A: "Behavioral equivalence is undecidable; CID equality is too strict."

Lemma 4 grants this. The catalog federates by syntactic CID equality; two behaviorally-equivalent algorithms with distinct specifications get distinct CIDs. The remedy is the explicit `refines` link in the lifecycle protocol (AMP §8.1): when a behaviorally-equivalent reformulation is desired, mint the new memento with a `refines` link to the prior CID. The catalog tracks the equivalence class explicitly. Federation does not require deciding behavioral equivalence; it requires only declaring it.

### Objection B: "The discharge protocol's prove portfolio is itself a substrate participant; this is circular."

It is, and the circularity is the point. The prove portfolio's outputs are content-addressed signed receipts. Discharging a binding-claim against an algorithm produces a receipt that is verifiable from its bytes alone, independent of the prove portfolio's continued operation. The portfolio is the *minting* mechanism for receipts; once minted, receipts stand independently. This is the same shape as the rest of the substrate: minting is dynamic, verification is byte-static.

### Objection C: "Language ports will resist the discipline; binding-claim minting is overhead."

The discipline replaces work that was already happening. Today, every language port reinvents pattern recognition algorithms by reading prior implementations (or reading the Rust reference and re-translating). That work is currently uncaptured: it produces language-specific source code with no canonical reference and no mechanically-detectable drift. Under AMP, the same work produces a binding-claim memento and a discharge receipt. The work is the same; the *artifact* is canonical instead of ephemeral. Language ports gain access to every other port's verified algorithms by reference.

### Objection D: "Self-hosting introduces a foundational regress: who verifies the verifier?"

The same answer as in After Verification (paper 07, §6): the prove portfolio's individual solvers are independently checkable. Z3 unsat cores can be verified by other tools. Coq proof terms can be re-checked. Vampire saturations can be replayed. The receipt's bytes carry the proof; checking is byte-mechanical and does not require trusting the minter. The portfolio is the *ecosystem* of independently-checkable proof systems; the substrate inherits their checkability.

### Objection E: "The catalog will explode in size as algorithms accumulate."

It will. So does git's object database; so does any content-addressed substrate. Asymptotic verification cost (per After Verification §6) approaches the cost of cache lookup, not the cost of full verification. The catalog's size is a function of how many distinct algorithms exist; the substrate's value scales with that count.

### Objection F: "Adding new language support requires writing the projection memento; this is friction."

Adding a new language port to *any* substrate requires bridge code. Without AMP, the bridge is per-pattern per-language source code that must be written for every algorithm. With AMP, the bridge is one projection memento per language; algorithms then attach by reference. Per-algorithm work is the binding-claim memento, which is small and verifiable. The friction is *redistributed*, not eliminated, but the redistribution favors compositionality.

## §6: What this enables

Four near-term consequences.

### §6.1 Cross-language verification federation becomes mechanical

A claim about a Python function's behavior, derived from a Python lifter's binding-claim against algorithm A, composes with a claim about a corresponding C function's behavior, derived from a C lifter's binding-claim against the same algorithm A. The composition is by algorithm CID. Cross-language reasoning over the same semantic content becomes an operation on the catalog, not bespoke per-pair plumbing.

### §6.2 Algorithm correctness becomes a single point of attestation

Today, the question "is the WP propagation correct?" has five answers (one per language). Tomorrow, it has one answer: the algorithm memento's specification, with five discharged binding-claims attesting that the language ports implement it. Drift between ports is a Lemma-3 violation, surfaced as a refuted discharge, attributable to a specific binding.

### §6.3 New patterns extend every language port simultaneously

When a new pattern is identified (say, a kernel idiom not yet in the catalog), minting one algorithm memento + one binding-claim per language gives every language port the new pattern. The work scales with language count, not with language-count × pattern-count.

### §6.4 The lifter family becomes auditable substrate

A consumer who wants to verify "the lifter that produced this contract is correct" can look up the binding-claim's discharge receipt, verify the receipt's bytes, and trust the contract's provenance. The lifter is no longer an opaque code artifact; it is a substrate participant with an attested correctness claim.

## §7: What this does *not* close

Several real questions remain open.

### §7.1 The canonical executable representation of algorithms

AMP v0.1.0 leaves `body_cid` optional (the canonical executable form of an algorithm). A future version must fix this. Candidates: Coq function terms, WebAssembly modules, lambda-calculus terms in a normalized syntax, or a purpose-built tree-transformation language. Each has tradeoffs (proof-checkability vs. broad portability vs. domain fit). The choice will determine how mechanically the discharge protocol operates and how decoupled the catalog is from any one prover.

### §7.2 ProofIR extensions for algebraic effects

Lemma 8 cites the algebraic-effects design, which is not yet implemented. The IR extensions (`EffectOp`, `Handler`, `ForallContinuation`, `Continuation`) and the effect-signature catalog are deferred to a separate spec and a separate paper. Until those land, AMP's coverage of interaction primitives is theoretical, not operational.

### §7.3 Migration of existing lifters

Currently no lifter has a binding-claim memento. The migration is substantial. AMP v0.1.0 specifies the protocol; the migration mechanism follows in a separate spec and is itself a multi-PR effort.

### §7.4 Catalog signing hierarchy

AMP v0.1.0 defaults all catalog mementos to the foundation v0 key. A future version may introduce delegated signing for language-port maintainers, with a chain-of-authority memento family.

### §7.5 Hyperproperties and probabilistic claims

The catalog mechanism extends naturally to algorithms whose specifications cite effect signatures; whether it extends naturally to hyperproperties (claims over pairs of executions) and probabilistic semantics is open. Likely answer: yes, via algorithm mementos whose post cites hyperproperty / probability-distribution effect signatures, but the construction needs working out.

## §8: The closing principle

The substrate's first axiom is *Supra omnia, rectum*. The substrate must apply that axiom to itself. Until today, it did not: the lifter family's algorithms were uncontrolled drift across language ports, no canonical reference, no content-addressed identity, no mechanical drift detection. The substrate produced canonical contracts via uncanonical machinery.

This paper's mechanism: algorithm mementos, binding-claim mementos, discharge receipts: closes the inconsistency. Federation by CID. Drift mechanically detectable. Soundness preserved across bindings. The catalog is Cousot's lattice of abstractions made content-addressed. The substrate's production mechanism becomes auditable substrate.

After Languages, every claim emitted by every lifter carries provenance to a content-addressed algorithm + a content-addressed binding-claim + a discharge receipt. The substrate's first axiom finally applies to its own machinery.

That is the move from substrate-as-output-discipline to substrate-as-self-hosted-discipline. After this paper's mechanism lands, the substrate is one structure all the way down.

T Savo
