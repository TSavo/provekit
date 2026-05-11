# Cross-language equivalence: how we know two `if`s are the same construct

ProvekIt namespaces every operation memento by language. The C11 conditional is `c11:if`. The C# conditional is `csharp:if`. The Rust one is `rust:if`. The JVM bytecode branch family is `jvm:ifz`. Each of those mementos is a `FunctionContractMemento` (per AMP/CCP), serialized with JCS, hashed with BLAKE3-512. The namespace prefix is part of the bytes that get hashed. So `cid(c11:if) ≠ cid(csharp:if)` by construction, before anyone looks at semantics.

That raises the obvious question. If sameness across languages cannot be CID equality (it never is, by design), then what *is* it? The answer is scattered: paper 13 has the lemmas, paper 16 has the colimit framing, paper 17 has the philosophy, the Language Signature Protocol (LSP) has the normative wire format, the concept-shapes menagerie has the running instances. This doc consolidates the mechanism, walks a concrete `if` example, gives the bytecode caveat, and maps the engineering prior-art this design sits on. It does not re-derive paper 13's lemmas; it summarizes and points.

## The question, stated plainly

`c11:if` and `csharp:if` are different CIDs. The namespace token (`c11:`, `csharp:`) is in the canonical bytes, so the hashes differ. Whatever "these two `if`s are the same construct" means, it cannot be `cid(a) == cid(b)`.

This is correct, not a workaround. Two reasons.

First, distinct CIDs preserve provenance. A `c11:if` came out of a C front-end's AST. A `csharp:if` came out of Roslyn. They sit in different surrounding semantics: C's `if (0)` coerces an integer to a truth value; C#'s `if` demands a `bool` and rejects the int. Collapsing them onto one CID would erase the fact that you are looking at C and not C#, and that distinction matters for exactly the cases where the two `if`s are not quite the same.

Second, CID equality would assert sameness by fiat. It would be true for the easy cases and silently false for the hard ones (the bytecode branch in §5, the conditional that triggers a different effect discipline, the language whose `if` is an expression versus the one whose `if` is a statement). The substrate's first axiom is *Supra omnia, rectum*. Sameness has to be *proved*, not stipulated, which means it has to be a separate, discharged, content-addressed fact, not a property of the address.

## The mechanism, in brief

(Full normative spec: [LSP v0.1.0](../../protocol/specs/2026-05-09-language-signature-protocol.md). Full theory: [paper 13](../papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md), Lemmas 1-5.)

Sameness is carried by a `LanguageMorphismMemento` (LSP §1.4) with a checkable discharge obligation. For a renaming morphism `m` between two operations, the obligation is, in shape:

```
φ_m(contract(c11:if)) ⊑ contract(csharp:if)   and the converse
```

where `contract(op)` is the op spec's `post`: the `operation-contract` carrying the `operator`, `arity`, `result`, `arity_shape`, the per-slot policy, and the `wp` transformer. The morphism's `φ` is a renaming that maps slot names, sorts, operators, and literals across the boundary. Same `arity_shape` is *necessary*. Agreement of the `wp`-transformers modulo `φ` is what is *demanded*. Discharge of that obligation uses the same machinery as any contract obligation: the canonicalizer when the morphism is renaming-and-representation only (alpha-equivalence plus a representation map: substitute, fold, canonicalize, compare CIDs), the prove portfolio (Z3, cvc5, Vampire, Coq) when the equational sub-obligations need a solver. A discharged morphism produces a signed `MorphismDischargeReceipt`. After that, `blake3-512:<morphism-cid>` is a citable fact: *these are the same construct, here is the proof, here is who signed it.*

The topology is hub-and-spoke, not pairwise. Each language's `if` maps to one hub shape (call it `concept:conditional`) by one morphism. Two languages' `if`s are "the same" iff they land on the same hub shape, or on hub shapes connected by a verified morphism. That is `N+M` morphisms, not `N²`. Pairwise sameness across any two languages is recovered by composing through the hub (LSP §6, paper 13 Lemma 2). The running instances of this pattern are the [concept-shapes menagerie](../../menagerie/concept-shapes/README.md) (six cross-language idioms, each with a shape CID, per-language realizations, and discharged morphisms) and the [`foo` algebraic shape](../../menagerie/foo-algebraic-shape/README.md) (the original one-node, four-lift exhibit: C, Rust, AArch64, x86-64 all collapsing to `lambda arg_0. ite(arg_0 == 0, -22, arg_0)`).

One caveat the doc owes you up front: the concept-shapes catalog today names *idioms* (`allocate-or-bail`, `check-bounds-then-access`, `acquire-use-release`, `validate-then-commit`, `branch-on-error-else-passthrough`, `refcount-inc-use-dec`), not primitive operations. There is no `concept:conditional` hub node yet. The `foo` shape is the closest existing artifact (it is the specialization of `branch-on-error-else-passthrough` where the guard is `x == 0` and the fail value is `-22`). Minting `concept:conditional` and the per-language `if → concept:conditional` morphisms is a follow-up this doc identifies, not something already in the catalog. The mechanism is fully specced; this particular hub node is not yet minted.

## Worked example: `if` across C, C#, Rust

The minted op specs are real artifacts. `menagerie/c11-language-signature/specs/op_if.spec.json`, `menagerie/csharp-language-signature/specs/op_if.spec.json`, `menagerie/rust-language-signature/specs/op_if.spec.json`. C11 and C# are byte-for-byte structurally identical modulo the `fn_name` prefix and the `locus`:

```jsonc
// c11:if (and csharp:if, modulo the namespace and locus)
"formals": ["cond", "then_branch", "else_branch"],
"formal_sorts": [Bool, Stmt, Stmt],
"return_sort": Stmt,
"pre": true,
"post": {
  "kind": "operation-contract",
  "operator": "if",
  "arity": ["Bool", "Stmt", "Stmt"],
  "result": "Stmt",
  "wp": "cond ? wp(then_branch, post) : wp(else_branch, post)",
  "arity_shape": { "kind": "named", "slots": [{"name":"cond"},{"name":"then_branch"},{"name":"else_branch"}] }
},
"effects": [{ "kind": "effect-polymorphic", "rule": "union(then_branch.effects, else_branch.effects)" }]
```

Read the `wp` clause carefully, because that is what discharge checks. `wp(if(cond, then_branch, else_branch), post) = cond ? wp(then_branch, post) : wp(else_branch, post)`. The transformer is *guarded*: under the assumption `cond`, you propagate the postcondition backward through `then_branch` only; under `¬cond`, through `else_branch` only. The dead branch contributes nothing. The substrate does not encode "the dead branch is unevaluated" as a separate per-slot field; it falls out of the structure of this transformer. (That is also why the `effects` rule is `effect-polymorphic` over the *union* of both branches' effects: statically, either branch could be the live one, so the conservative effect set is the join.)

The morphism `c11:if → concept:conditional` (and `csharp:if → concept:conditional`) is a `contract-renaming-morphism` whose `φ` maps the slot names `cond/then_branch/else_branch` onto the hub's slot names and the sort name `Bool` onto whatever the hub calls its boolean carrier. Its `homomorphism_obligation` is `canonicalizer-alpha-equivalence-plus-representation-map`: apply `φ`, canonicalize, check that the resulting CID equals the hub shape CID. Since C11 and C# differ only by the namespace prefix and the `locus`, after `φ` strips the namespace and the morphism drops the `locus` (it is not part of the `operation-contract`), the two land on byte-identical canonical payloads, hence the same CID, hence the same hub. The two `if`s are the same construct, and the receipt says so. Composing `c11:if → concept:conditional` with the inverse spoke gives `c11:if ≅ csharp:if` directly, pairwise, by Lemma 2.

Rust is the honest wrinkle. `rust:if`'s spec has the same `formals`, `formal_sorts`, `return_sort`, `pre`, `wp`, and `effects` as C11 and C#, but it is *missing the `arity_shape` field*. The Rust LSP mint has not been brought fully in line with the C11 and C# mints yet. So the Rust spoke morphism cannot today be the pure canonicalizer discharge that the C and C# ones are; it needs either the Rust spec updated to carry `arity_shape: {kind:"named", slots:[cond, then_branch, else_branch]}` (the obvious fix, since the `wp` clause already names those slots) or a morphism that supplies the slot shape as part of `φ`. This is one of the gaps in §7. It does not change the mechanism; it is alignment work on one minted memento.

## The functorial lift to terms

A morphism on *operations* lifts to a transport map on *terms*. Once `c11:if ≅ csharp:if` (as operations, via the hub and Lemma 2), the equivalence extends structurally:

```
c11:if(cond, then, else)  ≅  csharp:if(cond', then', else')
   iff  cond ≅ cond'  ∧  then ≅ then'  ∧  else ≅ else'   (each under the same morphism family, recursively)
```

That recursion *is* the transport map. It is a homomorphism of term algebras: lifting a morphism on the signature's operations to a map on the free term algebra over those operations is the universal-algebra construction (paper 13 Lemma 3, the initial-algebra universality). And it is exactly the mechanism by which a `.proof` written over a C program replays over its C# port: you do not re-run the prover; you transport the existing proof along the discharged morphism, term by term, and the obligations carry over because each operation's contract is preserved by the morphism (Lemma 4, soundness via homomorphism). Paper 13 §5.1 states this abstractly ("cross-language verification by morphism composition"). The concrete instance: `.proof` over `foo.c` → transport along `morphism_c_to_shape` → a proof over the shape → transport along `morphism_<lang>_to_shape⁻¹` → a proof over `foo.rs`, with no solver call across the boundary. ORP v0.2's round-trip theorem (paper 16 §6) is the same statement at the realizer layer.

This transport-to-terms is stated in paper 13 but is not yet a worked artifact in the menagerie. The `foo` exhibit transports *contracts* (the four lifts collapse to one shape contract); transporting a *proof* along the morphism is the next exhibit, not an existing one. Another §7 gap.

## The bytecode caveat: the case that proves the granularity earns its keep

`jvm:ifz` is not a source `if`. In the minted JVM bytecode signature it is one operation with slots `relation`, `value`, `target`: it is the algebra-level name for the whole `ifeq`/`ifne`/`iflt`/`ifge`/`ifgt`/`ifle` family, parameterized by the `relation` slot. (That parameterization is itself a small instance of the move: six bytecode mnemonics already lift to one signature operation.) But whatever you call it, `jvm:ifz` is a conditional *jump*: a `goto` (also in the signature, slot `target`) with a guard. It is control-flow-graph machinery, not a structured statement with a then-branch and an else-branch nested inside it.

So there is no direct morphism `jvm:ifz → c11:if`. There is `jvm:ifz → concept:conditional-jump` (a hub for guarded jumps), and then a *separate, strictly harder* morphism `concept:conditional-jump → concept:conditional` that holds **only for reducible control flow**: a CFG of guarded jumps recovers a structured `if`/`while` form exactly when it is reducible, by the structured-programming recovery result in the Böhm-Jacopini lineage. Reducibility is the morphism's *precondition*, part of its discharge obligation, not an assumption baked into an address. An irreducible CFG (the spaghetti `goto` mess, the loop with two entry points) does not discharge that morphism, and the substrate correctly refuses to call it the same construct as a source `if`.

This is the granularity earning its keep. The substrate distinguishes "the same construct" (source `if`s across C, C#, Rust, Go, which all discharge the cheap canonicalizer morphism into `concept:conditional`) from "decompiles-to the same construct under a hypothesis" (a bytecode branch, which discharges into `concept:conditional-jump` for free but into `concept:conditional` only under reducibility). CID equality would flatten both into one false identity. The morphism layer is exactly where the caveat lives, with the hypothesis as the precondition of a specific receipt. This connects to paper 9's lossy-boundary-compression theme: the bytecode-to-source recovery is a *lossy* boundary projection (the CFG carries layout the source `if` does not, and not all of it round-trips), and paper 9 is where the substrate's discipline for lossy boundaries lives.

## Why this is the right design: the prior-art map

This shape (namespaced per-language operations, plus structure-preserving maps between them as the dictionary, plus a hub topology) keeps getting independently reinvented. When a design gets discovered from five directions, that is evidence the design is right. Each entry below: who did it, what they have, what they are missing.

- **Compiler IRs, especially MLIR.** LLVM IR, GIMPLE, .NET CIL, WebAssembly are common concrete representations. MLIR goes further: its dialects are *namespaced* (`arith.addi`, `scf.if`, `affine.for`) and conversion passes lower between dialects. That is `c11:if`/`csharp:if`/`jvm:ifz` and the morphisms between them. *Missing:* the conversion passes are trusted code, not discharged obligations (a lowering pass is not a proof); the dialects are not content-addressed; lowering is one-directional and erasing. Closest *structural* precedent. (Paper 16 §1 reframes the whole compiler-IR history around this.)

- **Verified compiler correctness.** McCarthy and Painter 1967 (the first verified compiler). Morris 1973 ("compiler correctness is a commuting square: `⟦·⟧_src = ⟦·⟧_tgt ∘ compile`"). CompCert (Leroy). CakeML. Pilsner / PILS (Hur, Dreyer, et al.). The commuting square *is* "the morphism preserves the contract." *Missing:* hand-proved, one-source-to-one-target, monolithic; not a federated catalog of independently mintable, composable morphisms. (Paper 13 Lemma 5 and Objection B already name CompCert: the work CompCert did is the morphism discharge; what changes under LSP is that the artifact becomes a citable, composable receipt.)

- **Categorical and denotational semantics.** Reynolds, Scott, Plotkin: each language denotes into a shared category, two programs equal iff equal denotations, a translation is a functor commuting with the semantic functors. Goguen's institutions (the framework for hosting many logics at once). Mosses' action semantics. Plotkin and Power's algebraic effects (every effect is a Lawvere theory; handlers are homomorphisms). Mac Lane's categorical semantics. And the explicit metaphor: Baez and Stay, "Physics, Topology, Logic and Computation: A Rosetta Stone" (monoidal categories as the lingua franca, structure-preserving functors as the dictionary). *Missing:* a theory, not a running content-addressed federated tool. (Paper 13 §0 already names Goguen, Mosses, Plotkin-Power, Reynolds, Mac Lane; Baez-Stay is the one to add.)

- **Univalent foundations and mechanized transport.** HoTT's identity type: things are equal *by a path*, and there can be many paths. The colimit/orbit framing in paper 16 ("we discover the addresses; we do not invent them") is not an analogy for this, it is this. "Transport along an equivalence" is "replay the C proof over the C# port." Voevodsky's univalence ("isomorphic structures are equal") is "the shape's true name is its equivalence class," which is paper 17's "name by vector." The *mechanized* version: Isabelle's `transfer`/`lifting` packages (register a transfer relation, prove a transfer rule, theorems lift), Coq's univalent parametricity / Trocq (Tabareau, Tanter, Sozeau). *Missing:* within one prover, types not languages, not content-addressed, not federated.

- **Interlingua MT, ontology alignment, the K framework.** Old-school interlingua machine translation (translate each natural language to and from a meaning representation: `N+M`, not `N²`). Ontology and schema alignment via a shared upper ontology. Roşu's K framework (define `N` languages as rewrite theories in one logic, get tools for all of them). Same hub topology, same "the mapping is a checkable claim" structure. *Missing:* not content-addressed, not signed, the mappings are not first-class composable artifacts in a federation.

- **The deeper philosophy.** Leibniz's *characteristica universalis*: the universal symbolic language for reasoning. What Leibniz lacked was the canonicalization function that makes "same meaning" decidable; JCS-plus-a-hash is that function (paper 17 §1). Frege's sense and reference: the symbol is the reference, the dialect is the sense (paper 17 §2). These are pointed-to, not re-derived here.

- **The one inch that is ours.** Be honest: each piece above exists. MLIR has dialects-as-namespaces but no proofs and no hashes. CompCert has the correctness square but bespoke and monolithic. Isabelle's `transfer` transports proofs but inside one prover. HoTT has identity-as-structure but as foundations, not a running multi-language tool. Baez-Stay is a metaphor, not an artifact. ProvekIt is MLIR's namespaced dialects, plus CompCert's correctness-square as the discharge obligation, plus Isabelle-`transfer` as the proof replay, plus HoTT's orbit-as-the-name, plus content-addressing as the federation glue, all at once, over a signed substrate, where "two `if`s are the same" is a CID-addressable fact you can hand to a stranger who never coordinated with you. The parts are fifty years old; the bolt that holds them together is the new thing. Same pattern as the rest of the stack: Cousot 1977 is the math root, Schneier's *Applied Cryptography* chapter 1 is the design root, this is the federation root. *Caveat on the novelty claim:* to our knowledge this specific assembly is not done elsewhere. If there is a research system that does exactly this, it should be cited; this doc invites correction.

## What is still missing

Honest gaps:

- **No `concept:conditional` hub node.** The concept-shapes catalog names six idioms, none of them a primitive conditional. Minting `concept:conditional` plus the per-language `if → concept:conditional` morphisms is the obvious next exhibit.
- **`rust:if` is missing `arity_shape`.** The Rust LSP mint is not yet aligned with the C11 and C# mints. The fix is to add `arity_shape: {kind:"named", slots:[cond, then_branch, else_branch]}` to `rust:if`'s spec; the `wp` clause already references those slot names.
- **The functorial lift to terms is not a worked artifact.** Paper 13 states it; the `foo` exhibit transports contracts, not proofs. Transporting a `.proof` along a discharged morphism is the next menagerie exhibit.
- **The engineering prior-art synthesis here is not in the After-X papers.** Paper 13 §0 names the *mathematical* prior art (Goguen, Mosses, Plotkin-Power, Reynolds, Mac Lane). The *engineering* prior art (MLIR dialects, McCarthy-Painter / CompCert correctness squares, Baez-Stay, univalent transport / Isabelle `transfer`, interlingua MT / the K framework) is collected here for the first time. A candidate for folding into paper 13 §4 later.

## See also

- [Paper 13: After Grammars](../papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md): the canonical home. Lemmas 1-5 (signature CID identity, morphism composition, initial-algebra universality, cross-language soundness via homomorphism, compilation correctness as homomorphism discharge).
- [Paper 16: After Portability, the Universal Address Space](../papers/16-after-portability-the-universal-address-space.md): the colimit and orbit framing, "we discover the addresses; we do not invent them," the compiler-IR history reframe.
- [Paper 17: After Babel, We Speak in Vectors Now](../papers/17-after-babel-we-speak-in-vectors-now.md): Leibniz, Frege, name-by-vector, substitutability as a discharged path.
- [Paper 9: Lossy Boundary Compression](../papers/09-lossy-boundary-compression.md): the lossy-boundary discipline the bytecode-to-source recovery sits inside.
- [Language Signature Protocol (LSP) v0.1.0](../../protocol/specs/2026-05-09-language-signature-protocol.md): the normative spec for `LanguageSignatureMemento`, `LanguageMorphismMemento`, the homomorphism obligation, and discharge. The `provekit mint language-morphism` and `provekit mint language-signature` subcommands implement it.
- [Concept Shape Catalog](../../menagerie/concept-shapes/README.md): the running cross-language node table (the six idioms, their shape CIDs, realizations, and discharged morphisms).
- [Foo Algebraic Shape](../../menagerie/foo-algebraic-shape/README.md): the original one-node, four-lift exhibit (C, Rust, AArch64, x86-64 collapsing to one shape CID by canonicalizer discharge).

T Savo
