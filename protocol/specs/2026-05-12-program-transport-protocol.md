# Program Transport Protocol (PTP)

**Version:** v0.1.0 (draft)
**Date:** 2026-05-12
**Status:** design draft for review
**Author:** T Savo
**Companion specs:** LSP (2026-05-09-language-signature-protocol.md), Desugaring and the Core Compression (2026-05-11-desugaring-and-the-core-compression.md), AMP (2026-05-09-algorithm-memento-protocol.md), CCP (2026-05-09-contract-composition-protocol.md)
**Companion papers:** [paper 13: After Grammars](../../docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md), [paper 16: After Portability](../../docs/papers/16-after-portability-the-universal-address-space.md)

## §0: Why this spec exists

LSP defines language signatures and language morphisms. The desugaring spec defines how a language surface algebra collapses onto a smaller core algebra before crossing a language boundary. This spec defines the program-level transport that composes those pieces into a user-visible command:

```
lift(source file)
  -> desugar(source algebra term)
  -> transport-to-concept
  -> transport-to-target
  -> realize(target algebra term)
```

The protocol is deliberately refusal-first. A transport is valid only when every operation in the lifted and desugared source term has a discharged morphism into the concept hub, every concept operation has a discharged target-language spoke, and realization can emit a target artifact without changing the operation contract `wp`. If any condition is absent, the command returns a `Refusal` naming the missing extension.

## §1: Terms

- `source language`: the language of the input source file or input algebra term.
- `target language`: the requested output language.
- `concept hub`: the negotiated `concept:*` common-imperative operation set.
- `source term`: a term over the source language signature.
- `concept term`: the source term transported into the concept hub.
- `target term`: the concept term transported into the target language signature.
- `realized source`: target source text or another target artifact emitted from the target term.
- `discharged morphism`: a `LanguageMorphismMemento` minted from a real lifter-emitted source op whose `MorphismDischargeReceipt` has `discharged: true` under `canonicalizer-alpha-equivalence-plus-representation-map`.

## §2: Pipeline

### §2.1 Lift

`lift_L(source)` produces a term over `L` and records any source bytes required for lossless source-unit transport. A lifter can be a compiler front end, a language-specific Sugar lifter, or a loader for an already-lifted term.

Lift MUST refuse when the source cannot be parsed, when the requested function or unit is absent, or when the lifter cannot preserve enough source structure to build a term over the declared language signature.

### §2.2 Desugar

`desugar_L(term)` rewrites the lifted term with the language's discharged desugaring equation set `E_L`, as specified by the 2026-05-11 desugaring protocol. If no desugaring set is known for `L`, this stage is a no-op and the term remains partly surface. That is allowed, but the next stage will then require direct morphisms for any remaining surface operations.

Desugar MUST refuse if `E_L` is invalid, non-terminating, non-confluent, or lacks discharged `wp` preservation receipts. It MUST NOT apply an equation whose `wp` obligation is not discharged.

### §2.3 Transport to concept

For every operation `L:op` in the desugared term, the implementation looks up a discharged morphism:

```
L:op -> concept:op'
```

The morphism is a `LanguageMorphismMemento` whose `post.kind` is `contract-renaming-morphism`, minted directly from the real lifter-emitted operation spec for `L:op`. The morphism's `operator_map`, `renaming_map`, `representation_map`, and `literal_map` are applied to the entire operation contract, including `wp`, not just to the arity shape. The discharge obligation is:

```
canonicalizer-alpha-equivalence-plus-representation-map
```

The morphism discharges only when the canonical CID after substitution equals the target `concept:*` operation CID. If it does not equal, no morphism is minted; the op is recorded in `transport-gaps.md` with the structural reason and actual vs. expected values.

Transport to concept MUST refuse on the first operation without a discharged morphism.

### §2.4 Transport to target

For every operation `concept:op` in the concept term, the implementation looks up the target morphism in reverse:

```
T:op' -> concept:op
```

The protocol treats the reverse use as admissible only for discharged canonicalizer-equality morphisms: the source and target operation contracts are equal modulo the recorded maps, so the `wp` is preserved in both directions for that restricted operation contract. If the target language lacks such a morphism for a concept operation, transport MUST refuse.

### §2.5 Realize

`realize_T(target_term)` emits a target artifact. The artifact can be source text, target algebra IR, a proof bundle, or another declared target format. A source realizer SHOULD emit the target core form first. Re-sugaring into prettier surface syntax is explicitly outside the correctness obligation.

Realize MUST refuse if no target realizer exists, if the realizer cannot emit a construct without changing the target term, or if it would need a semantic guess.

## §3: Refusal taxonomy

Every refusal is a precise extension request. A refusal payload MUST include `kind`, `stage`, `language` when applicable, and enough operation or file context for a maintainer to add the missing fact.

### §3.1 Lift-time refusals

- `no-lifter-for-language`: no lifter adapter is registered for the source language.
- `source-file-not-found`: the requested file is absent.
- `parse-error`: the source language front end rejected the file.
- `unit-not-found`: the requested function, module, or declaration is absent.
- `source-language-mismatch`: an already-lifted term contains operations outside the declared source language.

### §3.2 Desugar-time refusals

- `no-desugaring-normal-form`: the desugaring set did not produce a unique core normal form.
- `non-terminating-desugaring-set`: the left-to-right rewrite system failed the termination gate.
- `non-confluent-desugaring-set`: the left-to-right rewrite system failed the confluence gate.
- `wp-preservation-not-discharged`: at least one equation lacks a discharged `wp` preservation receipt.
- `invalid-desugaring-equation`: a rule is not a valid desugaring equation memento.

### §3.3 Transport-time refusals

- `no-morphism-for-op`: a source operation has no discharged morphism into the concept hub.
- `no-target-morphism-for-op`: a concept operation has no discharged target-language morphism.
- `operation-cid-mismatch`: the term's operation CID does not match the discharged morphism row.
- `morphism-not-discharged`: a candidate morphism exists but lacks a discharged receipt.
- `wp-mismatch`: the substituted source operation contract does not canonicalize to the target operation contract.
- `roundtrip-closure-violation`: the target term transported back to concept is not the original concept term.

### §3.4 Realize-time refusals

- `no-realizer`: no target artifact emitter is registered.
- `unsupported-target-op`: the target realizer cannot emit a target operation.
- `would-change-term`: realization would need to change the target term.
- `proof-envelope-not-supported`: the term transport succeeded but proof envelope emission is not implemented for the requested target.

## §4: Morphism discharge requirements

A program transport implementation MUST NOT infer operation equivalence from names alone. The following must hold for every morphism used by PTP:

1. The source operation contract is a canonical AMP algorithm memento emitted by a real language lifter; it MUST NOT be a synthetic intermediate derived from the concept spec.
2. The morphism records the complete renaming, representation, operator, and literal maps.
3. Applying the maps to the source contract produces a canonical payload whose CID equals the target `concept:*` operation contract CID.
4. The equality covers `post.wp`, `post.arity_shape`, formal sorts, return sort, precondition, and effects in full.
5. The `MorphismDischargeReceipt` records `discharged: true` and method `canonicalizer-alpha-equivalence-plus-representation-map`.

The 12 idiom-shape morphisms retained from PR #604 (`menagerie/concept-shapes/specs/morphism_*_to_shape.spec.json`, whose sources are `menagerie/concept-shapes/sources/*.contract.json`) predate this protocol and use synthetic source contracts as a tracked pre-protocol exception, to be migrated to real lifter-emitted sources in a follow-up.

A failed discharge is a gap, not a warning. The generator MUST record the gap with the structural mismatch reason and actual vs. expected values, and MUST NOT mint the morphism.

## §5: Round-trip closure

For a source term `s`, source language `A`, and target language `B`, define:

```
c = transport_A_to_concept(desugar_A(lift_A(s)))
t = transport_concept_to_B(c)
c' = transport_B_to_concept(t)
```

PTP requires:

```
c' = c
```

where equality is structural equality over operation names, operation CIDs, variables, constants, and child terms. Implementations SHOULD additionally realize `t`, re-lift the emitted target source, and check that the re-lifted target term transports back to `c`. When a target source lifter is not available, the implementation MUST report that the closure was checked at the target-algebra term boundary, not at re-lifted source.

This property is the program-level counterpart of LSP morphism composition and paper 13 Lemmas 2, 3, and 4: discharged operation morphisms lift functorially to terms, and proof obligations transport along those lifted morphisms.

## §6: CLI binding

The user-facing commands are:

```
sugar transport <src> --to <target-lang> [--from <source-lang>] [--function <name>] [--out <dir>]
sugar migrate   <src> --to <target-lang> [--from <source-lang>] [--function <name>] [--out <dir>]
```

`migrate` is an alias when the intent is a source-language port. Both commands run the same PTP pipeline.

A successful JSON report SHOULD include:

- `status: "transported"`
- `source_language`
- `target_language`
- `stages`
- artifact paths for source term, concept term, target term, round-trip concept term, and realized target artifact
- morphism receipt references
- any deferred boundary, such as target source re-lift not being available

A refusal JSON report SHOULD include the taxonomy fields from §3.

## §7: Relationship to LSP and desugaring

PTP does not define a new language memento. It consumes LSP `LanguageSignatureMemento` and `LanguageMorphismMemento` records. Its operation transport table is the term-level lift of discharged LSP operation morphisms.

PTP does not define a new macro protocol. It consumes the 2026-05-11 desugaring equation sets. If a language has no set, PTP may still transport programs that already use only operations with direct concept morphisms.

PTP does not weaken `wp`. Every stage is obligated to preserve the operation contract `wp`; desugaring preserves it by equation discharge, morphisms preserve it by canonicalizer equality, and realization preserves it by emitting the target term without semantic mutation.

## §8: Bytecode caveat

The bytecode and assembly path is deferred. A bytecode operation such as `jvm:ifz`, `clr:brfalse`, `aarch64:cbz`, or `x86-64:jz` is not directly a `concept:conditional`. It is a conditional jump inside a control-flow graph. Recovering `concept:conditional` requires a reducible-control-flow precondition and a structured-control recovery pass:

```
ifz -> concept:conditional-jump -> concept:conditional
```

The middle node `concept:conditional-jump` is a useful documentation shape for the future bytecode path, but PTP v0.1.0 does not implement the recovery. A bytecode transport implementation MUST refuse unless it can prove the control-flow graph is reducible and the recovered structured term preserves the bytecode `wp`.

## §9: Re-sugaring caveat

Re-sugaring is a presentation search, not a correctness step. A target core term is already a valid target program when the target core is a subset of the target surface. Choosing a prettier target surface form, such as `foreach` instead of an iterator `while`, can be added later. Failure to re-sugar MUST NOT change the transport verdict.

## §10: Current v1 boundary

The v1 implementation mints the common-imperative concept op set, attempts to mint refinement morphisms from real lifter-emitted ops to concept hub ops via canonicalizer discharge, and transports C11 source terms to target algebras through the concept hub. Ops that do not discharge are recorded in `transport-gaps.md` with precise structural reasons. It emits core-form target source for the worked exhibit and checks concept round-trip closure at the target-algebra term boundary.

The following remain follow-up work:

- bytecode and asm control-flow recovery, as described in §8
- cosmetic re-sugaring, as described in §9
- adding new desugaring equation sets for languages that do not have one
- wiring every non-C source lifter into the CLI pipeline
- proof envelope emission for transported function contracts

T Savo
