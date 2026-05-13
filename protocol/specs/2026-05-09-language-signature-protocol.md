# Language Signature Protocol (LSP)

**Version:** v0.1.0 (draft)
**Date:** 2026-05-09
**Status:** design draft for review
**Author:** T Savo
**Companion specs:** AMP (2026-05-09-algorithm-memento-protocol.md), CCP (2026-05-09-contract-composition-protocol.md), PPP (2026-05-09-pattern-predicate-protocol.md)
**Companion paper:** [paper 13: After Grammars](../../docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md)

> **Naming note.** "LSP" in this document refers to the Language Signature Protocol. It does NOT refer to the Liskov Substitution Principle nor the Language Server Protocol. The substrate has used the abbreviation `LSP` informally for the language-server work; this spec claims the abbreviation for its own use within the protocol catalog. If conflict arises, the language-server work is referred to by its full name.

## §0: Why this spec exists

Programming language grammars are algebras (paper 13 §1). The substrate's catalog can host them. Doing so makes:

- Each language signature a content-addressed memento with a CID
- Each cross-language translation a morphism memento with a discharged homomorphism receipt
- Cross-language reasoning a composition of morphisms
- Compiler correctness a single attestable claim

This spec defines the wire formats, conventions, and protocols for hosting language signatures + morphisms in the substrate's catalog, building on AMP's algorithm-memento mechanism.

## §1: Definitions

### §1.1 Sort memento

A `SortMemento` describes a TYPE in a language signature.

```
SortMemento ⊆ FunctionContractMemento where:
  - fn_name           : the sort's canonical name (e.g. "Int", "String", "List<T>")
  - formals           : type parameters (empty for ground types)
  - return_sort       : the kind of the sort (e.g. * for ground, * → * for type constructors)
  - pre               : true (sorts have no preconditions)
  - post              : a description of the sort's structure (e.g. inductive constructors, denotation)
```

### §1.2 Equation memento

An `EquationMemento` describes an EQUATIONAL LAW over a signature.

```
EquationMemento ⊆ FunctionContractMemento where:
  - fn_name           : the equation's canonical name (e.g. "associativity-of-+")
  - formals           : universally-quantified variables
  - formal_sorts      : their sorts (CIDs of SortMementos)
  - return_sort       : Bool (an equation is a predicate)
  - pre               : optional context (e.g. "x ≠ 0" for a div-cancellation law)
  - post              : the equation as `lhs = rhs` where lhs and rhs are operations applied to formals
  - effects           : empty (equations are pure)
```

### §1.3 Language signature memento

A `LanguageSignatureMemento` describes a complete language as the bundle of its sorts, operations (algorithm mementos), equations, and effect signatures.

```
LanguageSignatureMemento ⊆ FunctionContractMemento where:
  - fn_name           : the language's canonical name + version (e.g. "rust:1.75.0", "c:c11", "python:3.12")
  - formals           : empty (a signature is a static description)
  - return_sort       : LanguageSignature
  - pre               : true
  - post              : the bundle structure:
                          {
                            sorts: [SortMemento_cid, ...],
                            operations: [AlgorithmMemento_cid, ...],
                            equations: [EquationMemento_cid, ...],
                            effect_signatures: [EffectSignatureMemento_cid, ...]
                          }
  - effects           : empty
  - body_cid          : OPTIONAL — CID of a reference grammar (BNF, formal-semantics document, ...)
```

### §1.4 Language morphism memento

A `LanguageMorphismMemento` describes a TRANSLATION between two language signatures, asserting the homomorphism property.

```
LanguageMorphismMemento ⊆ FunctionContractMemento where:
  - fn_name           : the morphism's canonical name (e.g. "rust-to-llvm-ir:rustc-1.75.0")
  - formals           : ["source_term"]
  - formal_sorts      : [TermInLanguage(source_signature_cid)]
  - return_sort       : TermInLanguage(target_signature_cid)
  - pre               : true (or restrictions on which source terms the morphism handles)
  - post              : the homomorphism obligation:
                          ∀ op ∈ source.operations.
                            morphism(source.apply(op, args)) =
                              target.apply(morphism_of(op), morphism_of(args))
                          ∧ for each equation e ∈ source.equations,
                            morphism(e.lhs) = morphism(e.rhs) holds in target
  - effects           : empty
  - input_cids        : [source_signature_cid, target_signature_cid]
  - body_cid          : CID of the morphism's implementation (compiler binary, translation table, etc.)
```

A morphism's discharge produces a `MorphismDischargeReceipt` memento (analogous to AMP's `BindingDischargeReceipt`) certifying that the homomorphism property holds.

### §1.5 Effect signature mementos

Per the algebraic-effects design (separate doc), each `EffectSignatureMemento` is itself a Lawvere theory:

```
EffectSignatureMemento ⊆ LanguageSignatureMemento where:
  - the sorts include the effect's input/output/resume types
  - the operations are the effect's operations (e.g. yield, send, acquire)
  - the equations are the effect's algebraic laws (e.g. send-receive cancel, lock-unlock pair)
  - effect_signatures field is empty (effect signatures don't recursively contain other effects)
```

By Lemma 6 of paper 13, effect signatures are special cases of language signatures, embedded as sub-signatures via the `effect_signatures` field of a containing `LanguageSignatureMemento`.

## §2: The homomorphism obligation, formally

Given:
- `S` : source language signature memento
- `T` : target language signature memento
- `M` : morphism memento with `M.input_cids = [S.cid, T.cid]`

The homomorphism obligation in `M.post` is:

```
∀ s ∈ Term(S).
  S.applies(s) → ∃ t ∈ Term(T). M(s) = t ∧ T.applies(t)
∧ ∀ op ∈ S.operations, ∀ args.
  M(S.apply(op, args)) = T.apply(M_op(op), [M(arg) for arg in args])
∧ ∀ eq ∈ S.equations.
  T.entails(M(eq.lhs) = M(eq.rhs))
```

Where:
- `M_op(op)` is the morphism's image of the operation `op` in T
- `Term(L)` is the term algebra over signature L
- `T.entails(...)` is provability in T's equational theory

This obligation is itself an IrFormula. Discharging it requires:
1. The prove portfolio (z3, cvc5, vampire, coq) for the equational sub-obligations
2. Coq for the structural induction over `Term(S)`
3. Hand-supplied morphism instances for non-trivial cases (the morphism's `body_cid` is the implementation; the discharge verifies the implementation satisfies the spec)

## §3: Catalog placement

Language signature mementos live at:

```
protocol/language-catalog/
  signatures/
    <lang_name>:<version>.<cid>.json
  morphisms/
    <source_lang>:<source_ver>__to__<target_lang>:<target_ver>.<morphism_cid>.json
  sorts/
    <sort_name>.<cid>.json
  equations/
    <equation_name>.<cid>.json
  index.json
```

The catalog has its own version (independent of the protocol catalog). LSP catalog v0.1.0 is the bootstrap.

## §4: Lifecycle

### §4.1 Minting a signature

To mint `LanguageSignatureMemento` `L`:

1. Identify `L`'s sorts, operations, equations, effect signatures.
2. Mint each sort memento individually (if not already in catalog).
3. Mint each operation memento via AMP (if not already in catalog).
4. Mint each equation memento.
5. Mint or reference effect signature mementos.
6. Build the bundle in `L.post`.
7. Sign with foundation v0 key (or delegated language-maintainer key).
8. Compute `L.cid`.
9. Add to catalog.

### §4.2 Minting a morphism

To mint `LanguageMorphismMemento` `M` from `S` to `T`:

1. Identify `M`'s implementation (compiler, translator, FFI binding).
2. Compute `M.body_cid` from the implementation bytes.
3. Build the per-operation mapping in `M.post`.
4. Build the homomorphism obligation per §2.
5. Sign with foundation v0 key (or delegated key).
6. Compute `M.cid`.
7. Add to catalog under morphisms/.

### §4.3 Discharging a morphism

The morphism's homomorphism obligation is an IrFormula. Discharge:

1. Lower to SMT-LIB or Coq via `provekit lower --to <target>`.
2. Run prove portfolio.
3. Receive verdict: UNSAT (negation of obligation has no model) = morphism is verified.
4. Mint `MorphismDischargeReceipt` with the portfolio's verdict and witness.
5. Sign and add to catalog.

### §4.4 Refinement

Signatures evolve via PEP. A signature edit (adding a sort, removing an operation, changing an equation) creates a successor memento `L'` with `L'.refines = L.cid`. Existing morphisms targeting `L` remain valid against `L`; new morphisms may target `L'`.

## §5: Federation rule

Two language signature mementos with the same CID are the same language by the substrate's definition. This is exactly Lemma 1 of paper 13.

Two language ports asserting they implement the same signature are mechanically asserting their grammars are byte-identical at the JCS canonicalized level. Any difference is detectable via CID mismatch.

## §6: Composition rule

Given discharged morphisms `M_AB : A → B` and `M_BC : B → C`, the composition `M_AC = M_BC ∘ M_AB : A → C` exists. Its CID is computed by the standard composition rule:

```
M_AC.cid = BLAKE3-512(JCS({
  composition_of: [M_AB.cid, M_BC.cid],
  source: A.cid,
  target: C.cid
}))
```

The composition's homomorphism obligation factors as the conjunction of the input morphisms' obligations. Discharging both inputs implies the composition discharges automatically.

This is Lemma 2 of paper 13.

## §7: Bootstrap path

LSP v0.1.0 is design. Implementation in sequence:

- **v0.2.0:** finalize `EquationMemento` shape, integrate with AMP catalog, define the `Term(L)` constructor for the substrate's IR.
- **v0.3.0:** mint signatures for ProvekIt's existing language ports (C/c11, Rust/1.75, Python/3.12, Java/17, Zig/0.13). Each signature is a JSON memento citing the sorts/operations/equations/effects from the existing lifters' implementations.
- **v0.4.0:** mint morphisms for known FFI boundaries (Python ↔ Rust via PyO3, Rust ↔ C via cbindgen, Java ↔ C via JNI, etc.).
- **v0.5.0:** discharge the morphism homomorphism obligations against the existing test corpora.

After v0.5.0, the substrate's federation operates at the language level. Cross-language verification reduces to morphism composition over the catalog.

## §8: Open questions

The following are intentionally NOT specified in v0.1.0:

- **`Term(L)` constructor** — the substrate's IR needs a way to denote terms in a specific language signature. Options: a new `IrTerm::TermInSignature { signature_cid, term_payload }` constructor, or a convention on naming. v0.2.0 must resolve.
- **Morphism implementation form** — `body_cid` references the compiler binary or translation table, but the substrate doesn't yet have a canonical way to verify "this binary implements this morphism." Two paths: empirical (run on a corpus, compare outputs) or formal (prove implementation against spec via Coq).
- **Higher-order signatures** — operations whose arguments are themselves operations require higher-order encoding. The substrate's catalog can host them; the prove portfolio's coverage of higher-order theories may need extension (Coq covers CIC; Lean / Agda would broaden).
- **Effect signature recursion** — can an effect signature's operations themselves invoke other effects? Algebraic effects literature says yes (handler composition); the catalog mechanism needs to support this without infinite recursion in CID computation.
- **Language version drift** — when language `L` evolves to `L'`, do existing morphisms targeting `L` automatically apply to `L'`? Only if the refinement preserves the morphism's image; this needs a check.

## §9: Out of scope (this draft)

- Implementing LSP. Design only. Reference implementation comes after review.
- Migrating ProvekIt's existing lifters to use LSP-aware machinery.
- Resolving the algebraic-effects ProofIR extensions (separate spec).
- Defining the canonical executable form for operations (deferred to AMP §11 resolution).

## §10: Why this matters (the closing principle)

The substrate's first axiom *Supra omnia, rectum* binds it. AMP closed the inconsistency at the algorithm layer. LSP closes it at the language layer. After LSP lands and is implemented, every claim about behavior — in any language, at any abstraction layer — settles into one content-addressed federated algebraic structure.

The substrate becomes the federation of programming language theory. Compiler correctness, cross-language verification, polyglot reasoning, and language design all reduce to operations over the catalog. The substrate's discipline applies one more level inward.

The first axiom finally applies to the LANGUAGES the substrate reasons about, not just the contracts written in them.

T Savo
