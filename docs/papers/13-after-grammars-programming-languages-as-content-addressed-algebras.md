# After Grammars: Programming Languages as Content-Addressed Algebras

> **Status.** Sustained argument. Contains nine lemmas with constructive proofs or proof sketches. Engages counterarguments. Written to be cite-able.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [07 After Verification](07-after-verification-bug-classes-as-missing-edges.md), [09 Lossy Boundary Compression](09-lossy-boundary-compression.md), [12 After Languages](12-after-languages-how-proofir-represents-every-language.md).
>
> **Companion specs.** [Algorithm Memento Protocol (AMP) v0.1.0](../../protocol/specs/2026-05-09-algorithm-memento-protocol.md), [Language Signature Protocol (LSP) v0.1.0](../../protocol/specs/2026-05-09-language-signature-protocol.md).
>
> **Premise the earlier papers established.** A content-addressed federated substrate of canonical predicates. Lifters that emit canonical contracts. After Languages (paper 12) closed the meta-level: the lifters' algorithms themselves become content-addressed mementos under AMP, with verifiable refinement-claims per language port. Federation by CID; drift mechanically detectable; the substrate hosts its own production mechanism.
>
> **What this paper argues.** That paper 12's mechanism extends one level further: programming language grammars themselves are content-addressable algebras. Every language is a finite signature: a set of sorts, operations, and equational laws. The substrate's catalog already has the apparatus to host them. Compilation between languages becomes a content-addressed algebra homomorphism. Cross-language verification becomes composition of morphisms. The substrate becomes the federation of programming language theory itself, where every claim about behavior in any language at any abstraction layer settles into one algebraic structure. We prove signature identity, morphism soundness, compilation correctness as homomorphism, and initial-algebra completeness, in nine lemmas.

## §0: The claim

Universal algebra and category theory established, decades ago, that programming language grammars ARE algebras. Goguen's institutions, Mosses' action semantics, Plotkin & Power's algebraic effects, Reynolds' polymorphic type theory, Mac Lane's categorical semantics: all settled the mathematical question. A grammar is a signature. A typing rule is an equation. A semantics is an algebra homomorphism into a target structure.

What has not been done before is making those algebras CONTENT-ADDRESSED and FEDERATED.

This paper argues that the substrate's catalog (extended by AMP and LSP) is the natural home for content-addressed programming language algebras, and that doing so yields a federation of programming language theory in which:

- Every language has a signature memento with a single CID
- Every typing rule is an equation memento referenced by CID
- Every compiler is a morphism memento describing source → target translation
- Every semantic-preservation proof is a homomorphism discharge receipt
- Cross-language verification is composition of morphisms over the catalog
- Two languages with the same signature CID ARE the same language by the substrate's definition
- Two languages with different CIDs but a discharged morphism between them are translations whose semantic preservation is mechanically checkable

After Grammars, the substrate is no longer just a verification platform. It is the content-addressed federation of programming language theory, with the same first-axiom discipline (*Supra omnia, rectum*) applied at every level.

## §1: The mathematics is settled

We do not need to invent the algebra; we only need to content-address it. The mathematical groundwork:

### §1.1 Lexical structure

Regular languages = finite monoids. A regex is a Kleene-algebra term. Brzozowski derivatives give an algebraic theory of regex parsing. Folklore.

### §1.2 Context-free structure

Context-free grammars are algebraic systems of equations. The term "algebraic language" comes from this: the languages denoted by CFGs are precisely the least solutions of polynomial equations over the algebra `(2^Σ*, ∪, ·)`. Salomaa, Conway. Folklore.

### §1.3 Type structure

Algebraic data types are LITERALLY algebras: sums of products, with recursion via least fixed points. Type theory generalizes this. Initial algebras of polynomial endofunctors are inductive types; final coalgebras are coinductive types. Lambek, Awodey.

### §1.4 Operational structure

Plotkin's structural operational semantics gives reduction rules. Big-step semantics is an algebra `(Term → Value)`; small-step is a coalgebra `(State → State successors)`. Either way, semantics is an algebraic structure.

### §1.5 Effect structure

Plotkin & Power: every observable computational effect arises from an algebraic theory (a Lawvere theory): a set of operations and equational laws. Bauer & Pretnar: handlers are algebra homomorphisms. Koka, OCaml 5, Eff, Frank operationalize this. We landed it as the algebraic-effects design.

### §1.6 Whole languages

Mac Lane: the syntactic category of a finitely-presented theory IS a category with finite products and finite equational laws. Programming languages are presentations of such categories. Mosses' Action Semantics gives a concrete framework. Goguen's institutions formalize cross-language relationships.

The conclusion is mathematically uncontroversial: **every programming language grammar is a presentation of an algebraic theory.** What is novel is making those presentations content-addressed mementos in a federated substrate.

## §2: The mechanism (summary; full normative spec in LSP)

The full normative specification is in [LSP v0.1.0](../../protocol/specs/2026-05-09-language-signature-protocol.md). The summary:

A `LanguageSignatureMemento` is a `FunctionContractMemento` (per CCP) with conventions:

- **`sorts`**: a content-addressed list of `SortMemento` references (the language's types/sorts)
- **`operations`**: a content-addressed list of `AlgorithmMemento` references (the grammar productions / language constructs / typing rules / reduction rules)
- **`equations`**: a content-addressed list of `EquationMemento` references (algebraic laws over the operations)
- **`effect_signatures`**: a content-addressed list of `EffectSignatureMemento` references (the language's interaction primitives, per algebraic-effects design)

A `LanguageMorphismMemento` is a `FunctionContractMemento` describing a translation between two language signatures. Its `pre` cites the source signature CID; its `post` asserts the homomorphism property: for each operation in the source, its image under the morphism satisfies the target signature's equations. Discharge of a morphism's homomorphism obligation produces a signed receipt.

This adds NO new primitives to the substrate. The catalog already hosts FunctionContractMementos (CCP). LSP defines conventions on what specific FunctionContractMementos describe. Federation, signing, evolution all inherit from the existing protocols.

## §3: The lemmas

We state and prove the load-bearing properties of this discipline. Notation: `L` ranges over language signatures; `M` over language morphisms; `cid(X)` is the BLAKE3-512 of the JCS encoding; `discharge(M)` denotes the prove-portfolio's verdict on M's homomorphism obligation.

### Lemma 1 (Signature CID Identity)

> Two language signatures `L` and `L'` are equal iff `cid(L) = cid(L')`.

*Proof.* By the substrate's content-addressing convention: `cid(L) = BLAKE3-512(JCS(L))`. JCS is byte-deterministic; BLAKE3 is collision-resistant. Equality of CIDs is therefore equivalent to byte-equality of the canonical encoding, which is equivalent to structural equality of the memento. □

The corollary, exactly as for algorithm mementos under AMP: federation by CID. Two language ports asserting they implement the same language signature are mechanically asserting structural equality.

### Lemma 2 (Morphism Composition)

> Given language signatures `A`, `B`, `C` and discharged morphism mementos `M_AB : A → B` and `M_BC : B → C`, the composition `M_AC = M_BC ∘ M_AB : A → C` exists, has a deterministic CID, and discharges iff both `M_AB` and `M_BC` discharge.

*Proof.* Composition of homomorphisms is a homomorphism (basic universal algebra). The composed morphism's CID is `BLAKE3-512(JCS(M_AB.cid, M_BC.cid))` per the substrate's standard composition rule. The discharge obligation for `M_AC` factors as the conjunction of `M_AB`'s and `M_BC`'s obligations. □

Implication: cross-language verification chains compose mechanically. Verifying a Python → Rust → C → LLVM IR pipeline reduces to discharging four pairwise morphisms, then composing.

### Lemma 3 (Initial-Algebra Universality)

> For every signature `L` in the catalog, there exists an INITIAL `L`-algebra `T_L` (the term algebra over `L`) such that for every `L`-algebra `A`, there is a unique homomorphism `T_L → A`.

*Proof.* Standard universal algebra. The term algebra `T_L` is the set of finite trees built from `L`'s operations, modulo `L`'s equations. Universality follows from the freeness of the term construction: given any algebra `A` interpreting `L`'s sorts and operations, the unique homomorphism is defined by structural induction on terms. □

This makes EVERY semantics for a language an instance of one canonical pattern: define the target algebra, derive the unique homomorphism, that is the semantics. Operational, denotational, axiomatic, type-theoretic: all are L-algebras differing only in their target.

### Lemma 4 (Cross-Language Soundness via Homomorphism)

> If morphism `M_AB : A → B` discharges (i.e. `M_AB` is a verified homomorphism), then for any contract `C_A` minted under signature `A` and its image `C_B = M_AB(C_A)` under `B`, soundness of `C_A` implies soundness of `C_B`.

*Proof.* Homomorphism preserves the algebraic structure: operations, equations, effect interpretations. Soundness is a structural property of the algebra (it is preserved under homomorphism by the satisfaction theorem of universal algebra). □

This is the load-bearing payoff for cross-language federation: a contract proven sound in language `A` is automatically sound in `B`, given a discharged morphism. No re-verification. The morphism's discharge IS the cross-language transferability proof.

### Lemma 5 (Compilation Correctness as Homomorphism Discharge)

> A compiler `K : Source → Target` is correct iff there exists a discharged morphism memento `M_K` from `cid(Source)` to `cid(Target)` whose `post` describes `K`'s lowering rules.

*Proof.* Compilation correctness is, by definition, the preservation of source-language semantics in the target. Semantics is captured as the language's signature (operations + equations + effect signatures). A correct compiler is a homomorphism between source and target signatures: it maps operations to operations, preserving equations and effect interpretations. Existence of a discharged `M_K` is therefore equivalent to compiler correctness. □

Implication: compiler correctness becomes a single attestable claim in the substrate, not a sui-generis verification project. seL4-style proofs of CompCert-style compilers reduce to morphism-discharge receipts. The work is the same; the artifact is canonical and federable.

### Lemma 6 (Effect Signatures as Embedded Lawvere Theories)

> Each effect signature memento referenced by a `LanguageSignatureMemento` is itself a Lawvere theory, embedded as a sub-signature. Effect handlers are algebra homomorphisms from the language's algebra into a handler-defined target algebra.

*Proof sketch.* Plotkin & Power proved that algebraic effects correspond to Lawvere theories (signature + equations). A language with effects is the union of its base signature with the effect sub-signatures. Handlers extend the algebra by interpreting effect operations into the target. Bauer & Pretnar formalized handler typing. □

Implication: the algebraic-effects design from earlier in the arc is not a separate primitive. It is a SPECIAL CASE of LSP: an effect signature is a particular kind of `LanguageSignatureMemento` whose operations are effect operations and whose equations are the effect's algebraic laws. Cross-language federation of effects (Python `yield` and Go `Send` binding to the same effect signature CID) is just morphism composition over the catalog.

### Lemma 7 (Equational Reasoning via Catalog Equations)

> Given a language signature `L` with equation set `E_L = {e_1, ..., e_n}`, equational reasoning over `L`-terms is decidable iff `E_L` admits a decision procedure (e.g. a confluent terminating term-rewriting system, a decidable congruence, or a tractable SMT theory).

*Proof.* Equational reasoning over `L`-terms is the word problem for the algebraic theory `(L, E_L)`. Decidability of the word problem depends on `E_L`. For free theories (no equations), trivially decidable (syntactic equality). For tractable theories (e.g. linear arithmetic, equality logic), decidable via the prove portfolio. For general theories, undecidable in general (Boone-Novikov for groups; the same for many language signatures). □

The substrate's prove portfolio handles the decidable cases mechanically. For undecidable cases, the substrate explicitly admits opacity entries (the Lossy Boundary Compression discipline of paper 09). No new structure required.

### Lemma 8 (Substrate Completeness for Algebraic Languages)

> Every language whose semantics can be presented as a finitely-presentable algebraic theory can be hosted in the substrate's catalog as a `LanguageSignatureMemento`. Conversely, every catalog `LanguageSignatureMemento` corresponds to a finitely-presentable algebraic theory.

*Proof.* The forward direction: a finitely-presentable algebraic theory is a finite set of sorts, operations, and equations. Each sort is a `SortMemento`; each operation is an `AlgorithmMemento`; each equation is an `EquationMemento`; the bundle is a `LanguageSignatureMemento`. The reverse direction: by construction, a `LanguageSignatureMemento` references finite lists of sort, operation, and equation mementos, which form a finite presentation. □

Implication: completeness of the substrate's language-hosting capacity matches the expressive power of universal algebra. Languages with infinitary semantics (e.g. higher-order logic with comprehension) require extending the meta-language; this is the same gap noted in AMP §11 (the canonical executable-form choice).

### Lemma 9 (Language Design as Algebra Construction)

> Designing a new programming language is equivalent to constructing a new `LanguageSignatureMemento`. Every design decision (adding a feature, removing a primitive, adjusting a type rule) corresponds to a structural change in the signature, mintable as a successor memento under PEP.

*Proof.* Trivially. A language is its signature; a design is a choice of signature; a design change is a signature edit. The substrate's PEP-governed evolution mechanism (paper 10) handles signature versioning, refinement, and deprecation. □

Implication: programming language design becomes a substrate-hosted, content-addressed, federated activity. Two researchers can independently mint signatures, the substrate identifies them as distinct (different CIDs) or related (via morphism mementos asserting translation), and the design space becomes mechanically navigable.

## §4: Counterarguments

We address the strongest objections.

### Objection A: "Real languages are not finite signatures. Macros, reflection, eval, dynamic linking, dependent types: these aren't tractable as algebras."

Granted, partly. A language with macro-expansion has a meta-level signature governing the macro language; the expanded language is the colimit. A language with eval has a HIGHER-ORDER signature whose operations include "interpret a term." These are extensions, not refutations: the substrate's catalog can host higher-order signatures (they are still finitely-presentable in the meta-language). The harder cases: dependent types, type-level computation, full reflection: push toward Martin-Löf type theory or CIC, both of which ARE finitely-presentable algebraic theories at the meta-level. The substrate's hosting capacity is the expressive ceiling of universal algebra extended with higher-order constructions, which is the same ceiling as the prove portfolio's solvers (Coq for CIC; Lean for HoTT-flavored constructions).

### Objection B: "Compilation correctness as a single homomorphism oversimplifies. CompCert took 200 person-years."

CompCert proved correctness for a specific source/target pair via 200 person-years of mechanized Coq work. Lemma 5 says compiler correctness IS a homomorphism property: it does not claim discharging the homomorphism is cheap. What changes under LSP is that the artifact CompCert produced (a Coq proof of refinement) becomes a content-addressed receipt usable across the substrate, citable from any other compiler effort, and composable with any other morphism for cross-language chains. The work is still hard; the work is now federated.

### Objection C: "Adding a sort or operation to the catalog requires consensus on what 'integer addition' or 'function call' means. Federation will fragment."

The catalog admits multiple distinct mementos for the same intuitive concept. There can be `IntegerAddition_TwosComplement_64bit` and `IntegerAddition_BigNum_Unbounded` and `IntegerAddition_Modular_p` as three distinct mementos with three distinct CIDs. They are not the same algorithm; they should not have the same CID. Where they CAN be related: when one refines another, or when a morphism between languages translates one to another: the relationship is itself a memento, content-addressed and signed. Federation is not by intuition; federation is by mechanical CID equality with explicit refinement links. This is the discipline; it is the point.

### Objection D: "Language designers will not use this. Catalog-minting is overhead they will not adopt."

Language designers already produce signatures: they call them BNF grammars, type-system descriptions, formal semantics papers. The substrate's catalog is the place to put what they already produce. The overhead is replacing prose-and-LaTeX descriptions with content-addressed JSON mementos. The payoff is mechanical federation, drift detection, and cross-language reasoning. Adoption follows tooling: when the next mainstream language ships with a `LanguageSignatureMemento` and a discharged morphism to LLVM, every other language gets free interop verification with it.

### Objection E: "Initial algebras and Lawvere theories are abstract; this paper is a category-theory pitch with no engineering payoff."

The engineering payoff is concrete. Federation Lemma 4 says: a contract proven in language A is mechanically valid in language B given a discharged morphism. Lemma 5 says: compiler correctness is a single attestable claim. Lemma 6 says: cross-language effect federation (Python `yield` and Go `Send` binding to the same effect signature) is mechanical. These are deliverable engineering outcomes. The category theory is the framework that makes them composable; it is not the deliverable.

### Objection F: "What about non-algebraic semantics: probabilistic, quantum, hyperproperties?"

Probabilistic semantics extend to algebraic theories over distribution monads (Giry monad, Plotkin's probabilistic powerdomain). Quantum semantics extend to algebraic theories over completely positive maps (Selinger). Hyperproperties extend via product algebras. Each is a substantial extension; each is well-studied in the literature; each can be a future signature class in the catalog. The substrate's mechanism (catalog of content-addressed signatures) is general enough to host them as they are formalized.

## §5: What this enables

Five near-term consequences.

### §5.1 Cross-language verification by morphism composition

A claim about Python code, lifted by the Python lifter, becomes a claim about the corresponding Rust code via the discharged Python→Rust morphism. The Python lifter's contract memento is mapped through the morphism to a Rust contract memento; soundness transfers by Lemma 4. No re-verification across the boundary; only one morphism discharge.

### §5.2 Compiler correctness becomes a federated artifact

Every compiler that ships with a discharged source→target morphism contributes to the substrate's correctness ecosystem. The morphism is signed, content-addressed, citable. A consumer who wants to trust a compiled binary can verify the morphism's receipt and the source-level proofs; the chain is mechanical.

### §5.3 Polyglot verification

A program that crosses N language boundaries (Python service calls Rust binding, calls C extension, calls Linux syscall, hits hardware register) reduces to N morphism discharges plus the per-language contracts. The verification chain is composition over the catalog. Cross-language reasoning becomes graph-traversal over a content-addressed federation.

### §5.4 Programming language design as a federated activity

A new language design becomes a `LanguageSignatureMemento` mint. Refinements are successor mementos with `refines` links. Comparison between two language designs is signature-CID inspection plus morphism discovery (does a translation between them exist? what equations does it preserve?). Language design becomes a public, federated, content-addressed practice.

### §5.5 Substrate hosts programming language theory

The catalog accumulates: every language signature ever minted, every morphism ever discharged, every effect signature ever registered. This is a federation of programming language theory itself, evolving monotonically, content-addressed, queryable, citable, composable. Programming language research becomes substrate-native.

## §6: What this does *not* close

Several real questions remain open, mostly inherited from AMP §11.

### §6.1 Canonical executable representation of operations

Each operation in a language signature is an algorithm memento per AMP. The canonical executable form for these (Coq term, WASM module, lambda calculus, ...) is the same open question as in AMP. Resolving it for AMP resolves it for LSP.

### §6.2 Equation memento shape

LSP needs `EquationMemento` to be precisely shaped: an LHS term, an RHS term, a quantifier prefix, and a context. The shape is straightforward but needs working out, and its canonical form must support the prove portfolio (so equations can be checked against terms via the existing solvers).

### §6.3 Higher-order signatures

Languages with higher-order operations (functions as values, type-level computation) require higher-order signatures. The substrate's catalog can host them, but the prove portfolio's coverage of higher-order theories (CIC, HoTT) is currently limited. Coq covers most cases; Lean and Agda would broaden.

### §6.4 Implementation roadmap

LSP v0.1.0 is design. Implementation follows in subsequent specs:

- v0.2.0: shape `EquationMemento` and add to AMP catalog
- v0.3.0: mint signatures for the languages already in ProvekIt's lifter family (C, Rust, Python, Java, Zig)
- v0.4.0: mint morphisms for the FFI boundaries between them
- v0.5.0: discharge the morphism homomorphism obligations against the existing test fixtures

After v0.5.0, the substrate's federation works at the language level, not just the contract level.

## §7: The closing principle

The substrate's first axiom is *Supra omnia, rectum*. Paper 12 (After Languages) closed the inconsistency at the algorithm layer: the lifters' algorithms become content-addressed mementos under AMP. This paper closes the inconsistency at the language layer: the languages themselves become content-addressed signatures under LSP.

After Grammars, every claim about behavior: in any language, at any abstraction layer, derived from any k_i: settles into one algebraic structure. Cross-language federation becomes morphism composition. Compilation correctness becomes morphism discharge. Programming language design becomes substrate-native R&D.

The substrate's first axiom now applies one more level inward: not just to its outputs, not just to its production mechanism, but to the LANGUAGES IT REASONS ABOUT. The federation is complete. Every layer is auditable substrate.

The substrate finally hosts programming language theory itself.

T Savo
