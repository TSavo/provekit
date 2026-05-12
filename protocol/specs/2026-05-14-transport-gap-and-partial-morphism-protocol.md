# Transport Gaps and Partial / Lossy Morphisms

**Version:** v0.1.0 (draft)
**Date:** 2026-05-14
**Status:** design draft for review
**Author:** T Savo
**Companion specs:** LSP (2026-05-09-language-signature-protocol.md), PTP (2026-05-12-program-transport-protocol.md), WPF (2026-05-13-wp-as-formula.md), CCP (2026-05-09-contract-composition-protocol.md), Desugaring and the Core Compression (2026-05-11-desugaring-and-the-core-compression.md), AMP (2026-05-09-algorithm-memento-protocol.md), Equational Portfolio Extension (2026-05-10-equational-portfolio-extension.md)
**Companion papers:** [paper 07: After Verification](../../docs/papers/07-after-verification-bug-classes-as-missing-edges.md), [paper 09: Lossy Boundary Compression](../../docs/papers/09-lossy-boundary-compression.md), [paper 13: After Grammars](../../docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md), [paper 16: After Portability](../../docs/papers/16-after-portability-the-universal-address-space.md), [paper 17: After Babel](../../docs/papers/17-after-babel-we-speak-in-vectors-now.md)

## §0: Why this spec exists

The cross-language transport layer mints an *exact* morphism `morphism_<lang>_<op>_to_<concept-op>` when a language's lifter-emitted op spec refines the corresponding `concept:*` hub op, and otherwise records a row in `menagerie/concept-shapes/transport-gaps.md`. That markdown file is, today, 463 lines of `| language | concept op | source spec | reason |`. It is the wrong representation for a structure the rest of the substrate has taught itself to take seriously: a precise, machine-readable, content-addressed fact about why two things that look like they should relate do not relate exactly, and what one could do about it.

The genuine semantic divergences the file records *should stay gaps*. `c11:add` is fixed-width modular arithmetic with undefined behavior on signed overflow; `python:add` is polymorphic, arbitrary-precision, and dispatches on operand sort. `c11:%` truncates toward zero; `python:%` is floored. `python:Int` is unbounded; `concept:Int` is fixed-width. `python:div` is true division; `concept:div` is integer division. Go and Zig short-circuit `&&` and `||` have no expression-position ternary to desugar through. Some languages genuinely lack an op. The substrate refusing to claim equivalence where there is none is correct, and *Supra omnia, rectum* requires exactly that refusal. PTP §3 names it: "a refusal is a precise extension request." But a row in a markdown table is not a precise extension request. It is a note. It is not content-addressed; it does not carry a structured reason a tool can branch on; it carries nothing about the resolution space; and "we considered re-speccing the hub op and chose not to" cannot be recorded against it at all.

This spec makes the negative space first-class. A transport gap becomes a `TransportGapMemento`: content-addressed, carrying the precise machine-readable reason it is a gap (a structured diff over `formal_sorts` / `pre` / `post` / `effects`, with a `gap_kind` enum and sub-tags), AND a menu of `resolution_options`, each option a real named structured thing with its precondition or characterization and its tradeoff, and an optional `status` so a project can record `accept-permanent` for `python:add → concept:add` as a signed decision rather than an unwritten one. And the corollary: the substrate gains the memento types those options reference. A `PartialMorphismMemento` is a morphism valid under a side-condition (a formula in the WPF formula language). A `LossyMorphismMemento` is a morphism into a *characterized coarsening* of the target. The discharge of either is a Z3 check in the same shape as the exact one (WPF §3), conditioned by the precondition or quotiented by the loss characterization.

And this is not transport-specific. A contract composition that does not cleanly compose is a *partial composition* (it composes under a precondition relating `A.post` and `B.pre`). A desugaring rewrite that *almost* preserves `wp` is a *lossy desugaring* (it preserves `wp` modulo a characterized difference, e.g. correct except on overflow). "An approximate relation between two things that should relate but don't relate exactly, honestly characterized" is a missing substrate primitive with three natural instantiations: morphism, composition, desugaring-rewrite. This spec names it, schematizes it for the morphism case, and sketches the other two.

### §0.1 Loss is the divergence-set, and a loudly-bounded lossy transformation is a first-class legitimate outcome

The word "lossy" sounds like degradation. It is not, and the design hinges on seeing why. `python:add` and `c11:add` are *literally the same function* on `{(a, b) : a + b ∈ [INT_MIN, INT_MAX]}` (signed), `1 + 1 ↦ 2` on both, and on the overwhelming majority of inputs any real program ever feeds an `add`. They part company at exactly one place: `INT_MAX + 1`, where `c11:add` is undefined behavior (or wraps to `INT_MIN`) and `python:add` keeps counting. The "loss" of a morphism `python:add → c11:add` is *precisely that complement set*: `{a + b ∉ [INT_MIN, INT_MAX]}`. A `LossyMorphismMemento.loss_characterization` is not a vague "approximate" tag, it is a precise formula naming the boundary at which two operations that mostly coincide stop coinciding. Use `1 + 1 ↦ 2` (canonical "they really do agree, here, on the cases that matter") and `INT_MAX + 1` (canonical "and here is exactly where they part") as the mental model.

So a *loudly-bounded* lossy transformation, a morphism / desugaring / composition that is correct *except* on a precisely-characterized failure set, shipped *with* (a) the recorded choice to use it, (b) the rationale, (c) the exact `loss_characterization` formula naming where it does not hold, is a **first-class legitimate outcome**, not a fallback or a degraded mode. It is *more* in the spirit of *Supra omnia, rectum* than a refusal would be: it ships something useful **and** is precisely honest about its domain of correctness. The `loss_characterization` formula *is* the contract of the lossy artifact, a lossy desugaring whose divergence-set is `S` is just a *contracted* rewrite: "these two terms agree, with precondition `¬S`", which is exactly what a `pre` is. The forbidden thing is *silent* loss: a lossy bridge with no characterization. That, and only that, is "claiming more than you can prove."

The real choice the substrate faces at a divergence is therefore a **trichotomy**, not refuse-vs-fake:

- **Exact**, best when available. The morphism / desugaring / composition discharges with `precondition = true` and `loss = ∅`.
- **Loudly-bounded-lossy**, the *common, legitimate case* for real cross-language ports and aggressive desugarings. Most real ports *are* characterizably lossy (every fixed-width target loses the unbounded source's large values; every floored-vs-truncated pair loses the mixed-sign cases; every float-vs-rational pair loses the non-representable reals). "Refuse → nothing ships" and "fake → silent bug" are both worse than "ship the bridge, name the edge, record the choice." This is where `LossyMorphismMemento` with `status: "chosen"` lives, a recorded, signed, *normal* decision, exactly like accepting a `pre`.
- **Refuse**, when you cannot even *characterize* the loss. If you can write the formula naming where it diverges, you are in the second case, not this one. This case is for "I do not know the boundary," not "the boundary is non-empty."

`PartialMorphismMemento` is the trichotomy's second branch presented from the *domain* side ("here is the sub-domain where it is exact"); `LossyMorphismMemento` is the same branch presented from the *codomain* side ("here is the quotient into which it is total"). They are dual views of one fact (§1.0), and which one a project instantiates is ergonomics, not semantics.

This is a design draft for review, not a decision. It defines the schemas, the discharge checks, the generator change, the CLI refusal change, and the migration path concretely enough to scope from.

## §1: The three memento types

### §1.0 The categorical reading, and the partial / lossy duality

An exact morphism is a structure-preserving map: `φ(<lang>:op)` lands on `concept:op`, every operation contract field preserved (paper 13's homomorphism obligation; PTP §4). A *partial* morphism is a span: a sub-object of the source language's terms (the use-sites where the precondition holds) over which a morphism exists, with the inclusion arrow into the full term algebra on one side and the morphism on the other. Equivalently, a partial function on terms, total on a characterized domain. A *lossy* morphism is a morphism into a quotient: you compose the would-be morphism with the quotient projection `concept:op → concept:op / ~`, where `~` is exactly the equivalence the `loss_characterization` collapses. The exact morphism is the degenerate case of both: precondition `true` (the span's sub-object is everything), loss `∅` (the quotient is trivial). This is paper 09's lossy-boundary-compression frame applied to the morphism layer: a boundary that loses information is honest only when the loss is characterized.

**The duality: restrict-domain ≡ quotient-codomain.** `PartialMorphismMemento` and `LossyMorphismMemento` are not two unrelated bolt-ons. They are two presentations of *one* gap. "Exact morphism `python:add → c11:add` restricted to `{a + b ∈ [INT_MIN, INT_MAX]}`" (the partial one, precondition `P`) and "total morphism `python:add → c11:add` that collapses the values where `c11:add` would overflow" (the lossy one, loss-characterization `¬P` where it bites) are the *same fact*, dual views of it: restricting the domain to where the morphism is exact is the same as quotienting the codomain by the distinction the morphism cannot preserve outside that domain. So the `TransportGapMemento` for a real semantic divergence records *both* presentations, the `PartialMorphismMemento` CID under precondition `P`, and the `LossyMorphismMemento` CID with `loss_characterization` `¬P`-where-it-bites, and "which one a project instantiates" is an *ergonomics* choice (refuse-unless-you-can-prove-`P`-at-every-use-site, versus proceed-with-recorded-loss), not a semantic one. Semantically there is one boundary; the two memento types are two views of it. That is what keeps them coherent: a divergence is one fact, recorded once, viewable from the domain side or the codomain side, instantiable either way.

### §1.1 `TransportGapMemento`

A `TransportGapMemento` records: operation `<lang>:op` (a CID) does not have an exact morphism into `concept:op` (a CID) because of a machine-readable reason, and here are the resolution options.

```cddl
; Imports:
;   ir-formula        ; from 2026-04-30-ir-formal-grammar.md, extended per WPF §2.3
;   cid               ; "blake3-512:" tstr

transport-gap-memento = {
  schema_version:    "1",
  kind:              "TransportGapMemento",
  fn_name:           tstr,                   ; canonical name, e.g. "gap:python:add:to:concept:add"
  source_op_cid:     cid,                    ; the lifter-emitted <lang>:op contract memento
  target_op_cid:     cid,                    ; the concept:op hub contract memento (or null when the gap is "no target op")
  source_lang:       tstr,                   ; e.g. "python"
  target_concept_op: tstr,                   ; e.g. "concept:add"
  gap_kind:          gap-kind,
  reason:            gap-reason,             ; structured diff; machine-readable
  ? reason_note:     tstr,                   ; OPTIONAL prose; non-load-bearing; MUST be omitted (not null) when absent
  resolution_options: [+ resolution-option],
  ? signature:       tstr / null             ; ed25519 over the canonical bytes; null in the unsigned exhibit
}

gap-kind = "sort-mismatch"
         / "polymorphic-source-op"
         / "divergent-semantics"
         / "missing-target-construct"
         / "missing-source-op"
         / "effect-mismatch"
         / "arity-shape-mismatch"
         / "wp-rule-mismatch"               ; post-WPF: the wp_rules do not refine; not a prose artifact

divergent-semantics-tag = "integer-vs-true-division"
                        / "truncated-vs-floored-modulo"
                        / "bounded-vs-unbounded-integer"
                        / "overflow-behavior"
                        / "rounding-mode"
                        / "short-circuit-vs-eager"
                        / tstr                ; open; a new tag is a new fact, recorded, not invented in code

gap-reason = {
  ? formal_sorts_delta: { got: [* ir-formula], want: [* ir-formula] },
  ? pre_delta:          { got: ir-formula,     want: ir-formula },
  ? post_delta:         { got: any,            want: any },        ; the operation-contract subtree that differs
  ? effects_delta:      { got: any,            want: any },
  ? wp_rule_delta:      { got: ir-formula,     want: ir-formula }, ; post-WPF
  ? divergent_tag:      divergent-semantics-tag,                   ; REQUIRED iff gap_kind == "divergent-semantics"
  ? source_supported:   bool                                      ; for missing-source-op: false, with the language's supported set as context
}

resolution-option = {
  option_kind:       resolution-option-kind,
  ? precondition:    ir-formula,             ; the side-condition under which a partial morphism would discharge
  ? characterization: ir-formula,            ; the divergence-set formula the lossy view's loss accepts
  ? loss_severity:   loss-severity,          ; for lossy-morphism: surfaced from the LossyMorphismMemento; advisory
  ? split_targets:   [+ tstr],               ; for split-target-op: the names the hub op would split into
  ? respec_target_to: any,                   ; for re-spec-target-op: the operation-contract the hub op would have to become
  ? representation_map_delta: any,           ; for add-representation-map: what φ's representation_map would need to carry
  ? partial_morphism_cid:  cid,              ; for partial-morphism: the PartialMorphismMemento (domain-side view), if computed
  ? lossy_morphism_cid:    cid,              ; for lossy-morphism: the LossyMorphismMemento (codomain-side view), if computed
  ? dual_view_cid:   cid,                    ; the OTHER presentation of the same divergence (a partial-morphism option points at the lossy CID and vice versa)
  tradeoff:          tstr,                   ; what you give up by taking this option
  ? status:          option-status           ; OPTIONAL; a project records its choice here, signs the memento
}

resolution-option-kind = "split-target-op"
                       / "partial-morphism"
                       / "lossy-morphism"
                       / "re-spec-target-op"
                       / "add-representation-map"
                       / "statement-level-desugaring"     ; the missing-target-construct escape: lower the expr-position op to a stmt + temp
                       / "accept-permanent"

option-status = "recommended"      ; the generator's suggestion
              / "chosen"           ; a project has selected this option (and, where applicable, minted the referenced memento)
              / "deferred"         ; under consideration, not acted on
              / "rejected"         ; considered and declined, with the tradeoff as the reason
```

`option_kind: "statement-level-desugaring"` is folded into the enum because the natural resolution for a `missing-target-construct` gap (Go / Zig `&&` with no expression-position ternary) is exactly "lower the expression-position op to a statement-level form with a result temporary," which is a desugaring move, not a morphism move; the option points the reader at the Desugaring spec, not at a `PartialMorphismMemento`.

The `gap-reason` `*_delta` fields mirror exactly what `mint_language_morphisms.py`'s `diff_reason()` already computes: `formal_sorts` first, then `pre`, then `return_sort` / `effects`, then `post.wp` (post-WPF: `wp_rule`), then `post.arity_shape`. The generator already has the structured comparison; this spec moves it from a `json.dumps`-into-a-prose-sentence into the memento's `reason` field.

The memento is JCS-canonical, BLAKE3-512-hashed, signed with the foundation v0 key (or a delegated project key when a project records a `chosen` / `rejected` status), and lives in the catalog alongside the morphisms it concerns (§4).

### §1.2 `PartialMorphismMemento`

A `PartialMorphismMemento` is a `LanguageMorphismMemento` (LSP §1.4) that holds *under a precondition*, a formula in the WPF formula language standing for a static fact about the source-program site where the op is used.

```cddl
partial-morphism-memento = {
  ; All LanguageMorphismMemento fields per LSP §1.4 / PTP §4, plus:
  kind:              "PartialMorphismMemento",
  fn_name:           tstr,                   ; e.g. "partial-morphism:python:add:to:concept:add"
  source_contract_cid: cid,                  ; the lifter-emitted <lang>:op contract
  target_shape_cid:  cid,                    ; the concept:op contract
  renaming_map:      any,
  representation_map: any,
  operator_map:      any,
  literal_map:       any,
  validity_precondition: ir-formula,         ; NEW; a formula over the op's formals + site-static predicates
  homomorphism_obligation: {
    kind:            "wp-refinement-under-precondition",
    source:          cid,
    target:          cid
  },
  ? gap_memento_cid: cid,                    ; OPTIONAL back-pointer to the TransportGapMemento that proposed this
  ? signature:       tstr / null
}
```

**What "valid under a precondition" means operationally.** `provekit transport` (PTP §2.3) may use a partial morphism *only* when it can establish `validity_precondition` holds at every use-site of the op in the lifted source term, statically, from the lift, with no runtime check inserted. If it cannot, it refuses, *and the refusal points at the gap memento*. A partial morphism is not a backdoor. It is an honest "this works iff P, and here is the P-check the pipeline must pass." For statically-typed languages the lift often carries enough sort information to discharge a precondition like `operands_statically_int`. For dynamic languages it usually does not (§6); in that case the `PartialMorphismMemento` exists in the catalog as a recorded option the pipeline cannot auto-use, which is still strictly better than a markdown row, it is a named, content-addressed bridge with a stated precondition, ready the day the lift learns to discharge it.

**The discharge.** A `PartialMorphismDischargeReceipt` certifies that the morphism's `wp_rule` refinement holds *conjoined with the precondition*. Concretely, for every postcondition `Q`:

```
validity_precondition  ⇒  ( wp(concept:op, Q)  ⇒  φ( wp(<lang>:op, Q) ) )
```

This is the WPF §3.2 check `wp(concept:op,Q) ⇒ φ(wp(lang:op,Q))` with `validity_precondition` added as a hypothesis. Same Z3, same `∀Q` handling (structural-match on `Q` plus a residual implication on the `pre` / value parts), same portfolio. When the precondition is `true` it collapses to the exact-morphism check (WPF §3.3), so the exact case is literally the `precondition = true` instance.

```cddl
partial-morphism-discharge-receipt = {
  schema_version:    "1",
  kind:              "PartialMorphismDischargeReceipt",
  morphism_cid:      cid,                    ; the PartialMorphismMemento
  source_contract_cid: cid,
  target_shape_cid:  cid,
  validity_precondition: ir-formula,
  obligation:        "wp-refinement-under-precondition",
  method:            "z3-quantified" / "structural-under-precondition",
  discharged:        bool,
  ? witness:         any,                    ; the portfolio's verdict / model, per the multi-solver protocol
  ? signature:       tstr / null
}
```

### §1.3 `LossyMorphismMemento` (a.k.a. `QuotientMorphismMemento`)

A `LossyMorphismMemento` is a `LanguageMorphismMemento` that holds only *after coarsening the target's contract*, you have decided to ignore some distinction the exact contract makes, and you have written down exactly which one. The canonical case: `python:add → c11:add` *if you quotient* the target's fixed-width `Int` by "agrees on all inputs whose sum fits the width," i.e. you accept that the transported program differs from the original *exactly* on the overflow set `{a + b ∉ [INT_MIN, INT_MAX]}` and nowhere else (`1 + 1 ↦ 2` on both; only `INT_MAX + 1`-class inputs diverge). This is the codomain-side view of the same fact the `PartialMorphismMemento` views from the domain side (§1.0); instantiating the lossy view with `status: "chosen"` is the *normal, legitimate* outcome for a real cross-language port, not a fallback.

```cddl
lossy-morphism-memento = {
  ; All LanguageMorphismMemento fields per LSP §1.4 / PTP §4, plus:
  kind:              "LossyMorphismMemento",
  fn_name:           tstr,                   ; e.g. "lossy-morphism:python:add:to:concept:add@mod64"
  source_contract_cid: cid,
  target_shape_cid:  cid,
  renaming_map:      any,
  representation_map: any,
  operator_map:      any,
  literal_map:       any,
  loss_characterization: ir-formula,         ; NEW; a formula exactly characterizing what the coarsening collapses (the rigorous part)
  loss_severity:     loss-severity,           ; NEW; a coarse advisory tag for HOW MUCH it loses (the heuristic part)
  coarsening_kind:   "quotient-target-sort" / "drop-target-precondition" / "widen-target-postcondition" / tstr,
  homomorphism_obligation: {
    kind:            "wp-refinement-into-coarsening",
    source:          cid,
    target:          cid
  },
  ? gap_memento_cid: cid,
  ? signature:       tstr / null
}

loss-severity = "loss-empty-on-bounded-subset"   ; exact whenever inputs stay in a bounded range; e.g. python:add → c11:add stays exact for all inputs whose sum fits
              / "loss-rare-in-practice"           ; the divergence-set is non-empty but real programs almost never hit it
              / "loss-common"                     ; the divergence-set is routinely hit; usable but the artifact is genuinely a different operation on a meaningful fraction of inputs
              / "loss-nearly-total"               ; the two coincide only on a negligible set; a red flag, almost certainly the wrong target (e.g. python:add → concept:string-concat)
```

The `loss_characterization` is a formula in the WPF formula language (the same grammar `pre`, `post`, `wp_rule`, loop invariants use), *where* it loses, the rigorous, solver-checked part. The `loss_severity` tag is *how much* it loses, heuristic, advisory, not a proof obligation, but a different and necessary fact: `python:add → c11:add` (lossy only on overflow, `loss-empty-on-bounded-subset`) and a hypothetical `python:add → concept:string-concat` (lossy ≈ always, semantically absurd, `loss-nearly-total`) have the *same shape* (a `LossyMorphismMemento` with a characterization formula and a discharge receipt) but are emphatically not the same kind of artifact, and the `--accept-loss` decision, and the gap memento's `recommended`-vs-`rejected` annotation on the option, needs both: the characterization to know exactly where it bites, the severity to know whether anyone should ever take it. For the mod-64 example the characterization is "`transported(x) ≡ original(x)  unless  |lhs + rhs| ≥ 2⁶³`" and the severity is `loss-empty-on-bounded-subset`. This is content-addressed like everything else; two lossy morphisms accepting different losses (or asserting different severities) have different CIDs, which is correct (PTP §4: name-equivalence is forbidden, contract-equivalence is what counts, and a coarsening *is* a contract change).

**The discharge.** A `LossyMorphismDischargeReceipt` certifies:

```
φ( wp(<lang>:op, Q) )  ⇒  coarsen( wp(concept:op, Q), loss_characterization )
```

read: the φ-translated lang op's `wp` is at least as strong as the *coarsened* concept op's `wp`. `coarsen(formula, L)` is the syntactic operation of weakening `formula` by exactly `L`, for `coarsening_kind: "quotient-target-sort"` it replaces equalities on the quotiented sort with the quotient-equivalence; for `"drop-target-precondition"` it deletes the named precondition conjunct; for `"widen-target-postcondition"` it disjoins the loss term into the postcondition. Each is a deterministic formula rewrite, content-addressed, then handed to Z3 in the WPF §3 shape. When `loss_characterization` is `false` (the empty loss), `coarsen` is the identity and the check collapses to the exact-morphism check, so the exact case is also the `loss = ∅` instance.

**What using a lossy morphism is, operationally.** It is an *explicit, recorded choice*. `provekit transport` will not use a lossy morphism unless the migration is invoked with `--accept-loss <loss-characterization-cid>` (or the loss is below a project-configured threshold, a coarseness metric over `loss_characterization`, e.g. "the disagreement set has measure zero" or "the disagreement set is a finite enumeration"). The produced transport artifact records, in its report, exactly which losses were accepted, by CID. Honest lossy is *recorded* lossy. Silent lossy is what the substrate refuses.

```cddl
lossy-morphism-discharge-receipt = {
  schema_version:    "1",
  kind:              "LossyMorphismDischargeReceipt",
  morphism_cid:      cid,
  source_contract_cid: cid,
  target_shape_cid:  cid,
  loss_characterization: ir-formula,
  coarsening_kind:   tstr,
  obligation:        "wp-refinement-into-coarsening",
  method:            "z3-quantified" / "structural-into-coarsening",
  discharged:        bool,
  ? witness:         any,
  ? signature:       tstr / null
}
```

## §2: Worked examples

Four gaps from the current `transport-gaps.md`, written as the mementos this spec proposes. Each is concrete enough to scope an implementation from.

### §2.1 `python:add → concept:add`, `polymorphic-source-op`

The current row: `python | concept:add | op_add.spec.json | precondition mismatch: got {true} want {no_signed_overflow(add(lhs,rhs))}`. The deeper truth: `python:add` is not `c11:add` with a weaker precondition, it is a *polymorphic* op (int add, string concat, list extend, depending on operand sort) over an *unbounded* `Int`. The gap memento:

```
gap_kind:          "polymorphic-source-op"
reason: {
  pre_delta:           { got: <true>, want: <no_signed_overflow(add(lhs,rhs))> },
  formal_sorts_delta:  { got: [Value, Value], want: [Int, Int] }   ; python's add takes Value, not Int
}
reason_note: "python:add dispatches on operand sort and operates on arbitrary-precision Int; concept:add is fixed-width modular with UB on signed overflow."
resolution_options: [
  { option_kind: "partial-morphism",           ; the DOMAIN-side view of the divergence
    precondition: <and(operands_statically_int(lhs), operands_statically_int(rhs), result_fits_64bit(add(lhs,rhs)))>,
    partial_morphism_cid: <blake3-512:...>,
    dual_view_cid: <the lossy-morphism CID below>,
    tradeoff: "exact at sites where the lift proves both operands are statically Int and the sum fits 64 bits; for un-annotated dynamic python that is almost never provable, so this stays a recorded option the pipeline cannot auto-use today.",
    status: "recommended" },
  { option_kind: "lossy-morphism",             ; the CODOMAIN-side view of the SAME divergence
    characterization: <implies(not(result_fits_64bit(add(lhs,rhs))), disagrees(transported, original))>,
    loss_severity: "loss-empty-on-bounded-subset",
    lossy_morphism_cid: <blake3-512:...>,
    dual_view_cid: <the partial-morphism CID above>,
    tradeoff: "the transported program agrees with the original everywhere except where the sum exceeds 64 bits (1+1↦2 on both; only INT_MAX+1-class inputs diverge); usable under --accept-loss; a recorded, signed choice, not a degraded mode.",
    status: "chosen" },                        ; ← a NORMAL, legitimate, signed state, like accepting a `pre`
  { option_kind: "accept-permanent",
    tradeoff: "decline any bridge: a polymorphic arbitrary-precision op is genuinely not a fixed-width modular op. Recording this as the project's standing choice is itself a signed decision, appropriate when even the loudly-bounded-lossy bridge is unwanted (e.g. the port must be all-or-nothing).",
    status: "rejected" }                       ; this project chose the loudly-bounded-lossy bridge instead
]
```

Note the duality (§1.0) in action: the partial-morphism option and the lossy-morphism option are not two separate fixes, they are the domain-side and codomain-side views of the *one* divergence (the overflow boundary), each pointing at the other via `dual_view_cid`. A project picks one to *instantiate*: this one picked `lossy-morphism` with `status: "chosen"` (proceed, record the loss, ship the port), which is the common legitimate outcome for a real cross-language port; another project that requires every use-site to prove no-overflow would instead instantiate the `partial-morphism` view. Same fact, two ergonomic stances.

### §2.2 `rust:rem → concept:mod`, `divergent-semantics:truncated-vs-floored-modulo`

The current row: `rust | concept:mod | op_rem.spec.json | precondition mismatch: got {nonzero(rhs)} want {true}`, but the precondition delta is the surface; the real divergence is that Rust's `%` (like C's) truncates toward zero, so `-7 % 3 == -1`, while a *floored* modulo would give `2`. `concept:mod` cannot be both. The gap memento:

```
gap_kind:          "divergent-semantics"
reason: {
  divergent_tag:  "truncated-vs-floored-modulo",
  pre_delta:      { got: <nonzero(rhs)>, want: <true> },
  post_delta:     { got: <... rem-truncated-toward-zero ...>, want: <... mod ...> }
}
reason_note: "rust:rem (and c11:%) truncate toward zero; a floored modulo rounds toward negative infinity. The two agree only when operands share a sign."
resolution_options: [
  { option_kind: "split-target-op",
    split_targets: ["concept:truncated-mod", "concept:floored-mod"],
    tradeoff: "the hub gains two ops instead of one; every existing concept:mod morphism re-targets to concept:truncated-mod (the C/Rust/Go/Zig family) and Python's floored % targets concept:floored-mod; clean, and the right call if more than one language family needs the other.",
    status: "recommended" },
  { option_kind: "partial-morphism",
    precondition: <same_sign(lhs, rhs)>,
    tradeoff: "valid only at sites where operands provably share a sign; rarely statically known.",
    status: "deferred" },
  { option_kind: "accept-permanent",
    tradeoff: "leave concept:mod meaning the truncated form (its current de-facto definition, per transport-gaps.md §Semantic Restrictions) and let floored-% languages keep a gap row until someone needs the split.",
    status: "deferred" }
]
```

### §2.3 `go:and → concept:?`, `missing-target-construct`

Go's `&&` short-circuits but Go has *no expression-position ternary*, there is nothing to desugar `concept:ite` into for the unevaluated right operand. The current `transport-gaps.md` records `concept:and | none` and `concept:ite | none`. The gap memento (for the Go spoke of `concept:and`):

```
gap_kind:          "missing-target-construct"
target_op_cid:     null                       ; the gap is "no target form in this language"
reason: {
  source_supported: true,                      ; go HAS &&; what it lacks is the expression-position desugaring target
}
reason_note: "Go && short-circuits, but Go has no expression-position ternary, so the standard ite-based desugaring of short-circuit && has no target. The same is true of Zig."
resolution_options: [
  { option_kind: "statement-level-desugaring",
    tradeoff: "lower `a && b` (in expression position) to `t := a; if t { t = b }; <use t>` at the statement level, correct, preserves wp, but is a desugaring move handled by the 2026-05-11 Desugaring spec, not a morphism. Requires the desugaring set to be confluent/terminating per that spec §2.2.",
    status: "recommended" },
  { option_kind: "accept-permanent",
    tradeoff: "Go and Zig short-circuit operators in expression position are recorded as a permanent gap until the statement-level desugaring set is minted; transport of programs using && in expression position refuses with a pointer to this memento and the desugaring option.",
    status: "deferred" }
]
```

### §2.4 `python:div → concept:div`, `divergent-semantics:integer-vs-true-division`

`concept:div` is integer division (per `transport-gaps.md` §Semantic Restrictions); Python's `/` is true (float) division and `//` is integer. The current row for `python | concept:div` is `not-supported`, python's lifter doesn't emit a `div` at all under the current generator config, which is itself a polite fiction; the real story is "Python `/` is a different operation." The gap memento:

```
gap_kind:          "divergent-semantics"
reason: {
  divergent_tag:  "integer-vs-true-division",
  post_delta:     { got: <... true-division on Value ...>, want: <... integer division on Int ...> }
}
reason_note: "python `/` is true division yielding a float; concept:div is integer division. python `//` is integer division but on arbitrary-precision Int."
resolution_options: [
  { option_kind: "split-target-op",
    split_targets: ["concept:int-div", "concept:true-div"],
    tradeoff: "the hub distinguishes the two; python:// targets concept:int-div (modulo the unbounded-Int gap, §2.1), python:/ targets concept:true-div; matches how Python itself spells them.",
    status: "recommended" },
  { option_kind: "partial-morphism",
    precondition: <and(operands_statically_int(lhs), operands_statically_int(rhs), divides_evenly(lhs, rhs))>,
    tradeoff: "python `/` matches integer division only when the operands are statically Int and the divisor divides evenly; almost never statically provable. Recorded option, not auto-usable.",
    status: "deferred" },
  { option_kind: "accept-permanent",
    tradeoff: "leave python without a concept:div spoke; record the reason here rather than as a `not-supported` row that hides it.",
    status: "deferred" }
]
```

These four exhibit, between them, every `gap_kind` that carries a non-trivial resolution menu (`polymorphic-source-op`, `divergent-semantics` with two sub-tags, `missing-target-construct`), and every `option_kind` except `re-spec-target-op` and `add-representation-map`, those two are the natural options for, respectively, an `arity-shape-mismatch` (the hub op's slot policy is wrong, re-spec it) and a `sort-mismatch` (the morphism's `representation_map` is missing a `φ` entry that would canonicalize the sorts), and the spec does not need a fifth worked example to make them clear.

## §3: The generator change

`mint_language_morphisms.py` (or its successor) changes its per-`(lang, concept-op)` decision:

1. If the canonicalizer discharge lands on the concept shape CID, exact morphism, as today: mint the `MorphismMemento` + `MorphismDischargeReceipt`.
2. Else if the structural relaxation discharges (today: `wp-text abstraction` / `pre-weakening`; post-WPF: the WPF §3 refinement check), exact morphism with the relaxed method, as today.
3. Else, instead of a `transport-gaps.md` row, emit a `TransportGapMemento`. The generator computes `gap_kind` and the `reason` deltas from the same `diff_reason()` comparison it already does (it has the structured `after_spec` and `concept_spec` in hand; it currently `json.dumps`es the diff into a sentence). It populates `resolution_options` from a per-`gap_kind` template:
   - `sort-mismatch` → `[add-representation-map (with the representation_map_delta the generator can read off the two formal_sorts), accept-permanent]`.
   - `polymorphic-source-op` → `[partial-morphism (the generator *derives* the precondition under which a sort-mismatch or pre-mismatch would discharge, e.g. `operands_statically_int` from a `Value`-vs-`Int` formal-sorts delta), accept-permanent]`, and where the source op is also unbounded, a `lossy-morphism` option with the mod-2ⁿ characterization.
   - `divergent-semantics` → `[split-target-op (with the two target names, from a small per-sub-tag table: `truncated-mod`/`floored-mod`, `int-div`/`true-div`, ...), accept-permanent]`, plus a `partial-morphism` option when there is a derivable side-condition (`same_sign`, `divides_evenly`).
   - `missing-target-construct` → `[statement-level-desugaring, accept-permanent]`.
   - `missing-source-op` → `[accept-permanent]` (a language genuinely lacking an op has no morphism resolution; the only options are "add the op to the language", out of the substrate's gift, or accept it).
4. For each `partial-morphism` option whose precondition the generator can *derive* and whose conditioned refinement it can *discharge* structurally, it also mints the `PartialMorphismMemento` + `PartialMorphismDischargeReceipt` and back-references it from the option (`partial_morphism_cid`). `LossyMorphismMemento`s are minted only when the project's mint config opts into a specific coarsening (the generator does not invent losses; it proposes them in the option's `characterization` field and mints the memento only on opt-in).
5. `menagerie/concept-shapes/transport-gaps.md` becomes a *rendered view* over the gap-memento + partial/lossy-morphism catalog, a generated table, like a docs page: the gap rows are now `| language | concept op | gap_kind | gap memento CID | resolution options |`, each option a one-line summary with its status. The file stays in the repo (so the at-a-glance table survives) but its source of truth is the memento catalog, not the generator's in-memory `gaps` list. The `## Semantic Restrictions` prose stays (it is editorial framing, not a gap).
6. Deterministic, content-addressed, the usual `mint.sh` re-run byte-clean property: same lifter specs in, same gap-memento CIDs out, same rendered view.

The generator stops being the only place the gap reason lives. It becomes a *producer* of gap mementos, the same way it is a producer of morphism mementos.

## §4: Catalog placement

Gap and partial/lossy-morphism mementos live alongside the morphisms they concern:

```
menagerie/concept-shapes/
  specs/
    gap_<lang>_<op>_to_<concept-op>.spec.json            ; TransportGapMemento
    partial_morphism_<lang>_<op>_to_<concept-op>.spec.json
    lossy_morphism_<lang>_<op>_to_<concept-op>@<tag>.spec.json
  receipts/
    partial_morphism_<lang>_<op>_to_<concept-op>.receipt.json
    lossy_morphism_<lang>_<op>_to_<concept-op>@<tag>.receipt.json
  catalog/
    algorithms/  receipts/  gaps/                          ; gaps/ holds the gap memento CIDs
  cids.tsv                                                 ; gains `gap` and `partial-morphism` / `lossy-morphism` kind rows
  transport-gaps.md                                        ; now a generated VIEW, not the source of truth
```

For language-level (non-menagerie) use, the LSP catalog (LSP §3) gains the parallel directories `partial-morphisms/`, `lossy-morphisms/`, `gaps/` under `protocol/language-catalog/`. The CID rules, canonicalizer, hash, and signature discipline are LSP's, unchanged.

## §5: The transport CLI / refusal change

PTP §3.3's refusal taxonomy gains one kind and the existing `no-morphism-for-op` / `no-target-morphism-for-op` refusals get a richer payload. When a lifted IR term contains a `<lang>:op` with no *exact* discharged morphism into the hub, `provekit transport` / `provekit migrate` no longer returns the bare:

```json
{ "kind": "no-target-morphism-for-op", "stage": "transport-to-concept", "language": "python", "op": "python:add" }
```

It returns:

```json
{
  "kind": "transport-time:gap",
  "stage": "transport-to-concept",
  "language": "python",
  "op": "python:add",
  "gap_memento": "blake3-512:...",
  "options": [
    { "option_kind": "partial-morphism", "partial_morphism": "blake3-512:...",
      "precondition": "operands_statically_int(lhs) ∧ operands_statically_int(rhs) ∧ result_fits_64bit(add(lhs,rhs))",
      "pipeline_can_establish": false,
      "note": "the lift does not carry enough static sort info to discharge the precondition for dynamic python; pass a sort-annotated source or use a different option" },
    { "option_kind": "lossy-morphism", "lossy_morphism": "blake3-512:...",
      "loss_characterization": "disagrees on inputs where |lhs + rhs| ≥ 2^63",
      "to_use": "re-invoke with --accept-loss blake3-512:<loss-characterization-cid>" },
    { "option_kind": "accept-permanent",
      "note": "no exact bridge; this gap is, by the project's recorded decision, permanent" }
  ]
}
```

So the refusal *is* the precise extension request the principle promises: it names the gap memento, enumerates the resolution options, says for each partial morphism whether the pipeline can establish the precondition (and if not, why), and says for each lossy morphism the exact flag and loss CID the user would supply to accept it. The PTP §3.3 entry:

- `transport-time:gap`: a source operation has no exact morphism into the hub; the payload names the `TransportGapMemento` CID and the resolution options, including any `PartialMorphismMemento` (with whether the precondition is establishable from the lift) and any `LossyMorphismMemento` (with the `--accept-loss` invocation). This *replaces* `no-morphism-for-op` / `no-target-morphism-for-op` as the standard refusal for a missing exact morphism; those kinds remain only for the degenerate case where not even a gap memento has been minted yet (a brand-new op the generator has not run over).
- `transport-time:gap-over-budget`: a source operation has a `PartialMorphismMemento` or `LossyMorphismMemento` available, but its precondition cannot be established at the use-site and its loss is not within the caller's loss-budget (§5.1). The payload names the op + source location, the `TransportGapMemento` CID, and `minimal_additional_budget`: the formula that, if added to the budget, would close this site. The dead end tells the caller exactly the price of getting past it.

### §5.1 The loss-budget: the gap is a negotiation, not a dead end

The reason `loss_characterization` MUST be a *formula* and not just a severity tag is that a formula is the only representation you can **intersect, compare by `⊆`, propagate through dataflow, and Z3-check against a budget**. (`loss_severity` is a UX/advisory convenience layered on top; the formula is what makes the mechanism computable.) That representation choice turns a gap from a stop sign into an optimization problem with three operational consequences:

**(1) Loss-budget as a first-class input.** The caller, or the project recorded as a memento, specifies what divergences are tolerable: a formula (`tolerate {overflow on values past 2³¹}`, because the program's domain is bounded), or a severity threshold, or `exact-only` (the budget is `false`, no loss admitted). `provekit transport` then *solves*: find a morphism set for the ops the program uses that minimizes a cost (prefer exact `>` partial `>` lossy; among lossy, prefer smaller divergence-sets), subject to `loss_i ⊆ ¬budget` for each op `i`. This is a Z3-checkable constraint solve over content-addressed formulas. The candidate morphisms for a given op order by `⊆` on their losses (`loss_A ⊆ loss_B ⇒ A is strictly better`), so the search is principled, not heuristic. New CLI surface: `--loss-budget <formula-cid>` (or `--loss-budget-memento <cid>` to point at a recorded project budget), with `--accept-loss <cid>` / `--accept-loss-threshold <metric>` from above being shorthand for narrower budgets.

**(2) Loss composes through dataflow, so the transported *program* gets a fidelity-domain.** The transported program's total divergence-set from the original is derivable from the per-op `loss_characterization`s plus the program's dataflow, this is *the same operation `compose` runs to propagate `pre`s*, performed on the complementary side of the contract (a `pre` is "where the op is defined"; a loss is "where the op's transport disagrees"; both propagate backward through dataflow by the same machinery, which is exactly why this spec and #613 are one piece of machinery seen from two sides of the contract). So `provekit migrate` reports, *before running anything*: "this port is exact except on inputs satisfying `{add@L42 overflows the C width} ∨ {div@L70 negative-operand case}`", and that **fidelity-domain ships with the produced artifact**, the way a `pre` ships with a contract. The port is not "approximate"; it is "exact, with this precondition", a `pre` on the whole transported program, content-addressed, computed not asserted.

**(3) The refusal is constructive.** When the budget cannot be met, the result is not `Refusal{no-morphism}` but `Refusal{kind: "transport-time:gap-over-budget", op_site: <op + source location>, gap_memento: <cid>, minimal_additional_budget: <the formula that, tolerated, closes it>}`. The caller learns the exact additional tolerance the port would cost, and can decide to widen the budget (a recorded decision), pick a different op-resolution, or accept the refusal. A dead end that quotes its own price is an extension request in the strongest sense PTP §3 means.

A new flag on `provekit transport` / `provekit migrate`: `--accept-loss <cid>` (repeatable), pre-authorizes the named `loss_characterization` CIDs; the pipeline may then use any `LossyMorphismMemento` whose loss is in the authorized set. A `--accept-loss-threshold <metric>` variant authorizes all losses below a coarseness bound. The transport report's `stages` block records, per stage, which lossy morphisms were used and which losses were accepted, by CID, plus the accumulated program-level fidelity-domain formula, the artifact is self-describing about its own coarsening.

**Round-trip closure with loss.** PTP §5 requires `c' = c` (concept round-trip). A program transported via a partial or lossy morphism does *not* satisfy that with equality, and that is correct, that is the loss being visible. The closure obligation becomes: a program transported via lossy / partial morphisms re-lifts and transports back to a *contracted* concept IR `c'` whose precondition is exactly the accumulated fidelity-domain `L` from (2), `c'` is `coarsen(c, L)`, the round-trip lands on the coarsened concept term, and the diff `c \ c'` is exactly `L`. The transport report states `roundtrip_closure: "coarsened-by <fidelity-domain-cid>"` rather than `"exact"`. A program transported entirely via partial morphisms used at sites where their preconditions hold *does* satisfy exact closure (each precondition restricts to the sub-domain where the morphism is total and exact); the report states `roundtrip_closure: "exact-on-fidelity-domain"` and the fidelity-domain is the conjunction of those preconditions. This is the "honest lossy ≠ silent lossy" guarantee at the round-trip layer: you cannot transport-with-loss and then claim an exact round-trip; the loss shows up in `c'` as a `pre`.

## §6: The generalization, one primitive, three instantiations

The "approximate relation between two things that should relate but don't relate exactly, honestly characterized" is not transport-specific. Two more instantiations, sketched (the morphism case in §1 is the worked one; these get a paragraph each, no CDDL, the schema shape is the obvious analogue: a `*Memento` plus a `precondition` or `characterization` formula in the WPF language, plus a discharge receipt that is the exact-case Z3 check conditioned or quotiented).

**`PartialCompositionMemento`.** CCP composes two contracts `A` and `B` by checking `A.post ⇒ B.pre` (modulo renaming). When that does not hold cleanly but holds *under a precondition relating the two*, say `A` and `B` compose whenever `A`'s output buffer is non-null, a fact about `A`'s caller context, that is a *partial composition*. The memento is the would-be `CompositionMemento` plus a `composition_precondition` formula; the discharge is `composition_precondition ⇒ (A.post ⇒ B.pre)` checked by the same Z3 CCP uses, and the composed contract is usable only at call-sites where the precondition is established (exactly the partial-morphism use-site discipline, one layer up). The exact composition is the `precondition = true` case.

**`LossyDesugaringMemento`.** The Desugaring spec §1.2 requires a desugaring equation `op(x...) = e` to satisfy `wp(op(x...), Q) ≡ wp(e, Q) ∀Q`, and refuses to call a `wp`-changing rewrite a desugaring. But some rewrites *almost* preserve `wp`: the classic `x + y` → `x | y` peephole when both are known small, or a desugaring that is correct except on overflow. A *lossy desugaring* is a rewrite rule that preserves `wp` modulo a characterized difference: the memento is the would-be `DesugaringEquationMemento` plus a `loss_characterization` formula (the exact set of states on which the two sides' `wp`s disagree), and the discharge is `wp(lhs, Q) ⇔ coarsen(wp(rhs, Q), loss_characterization)`, the WPF bi-implication, quotiented. It is *not* admissible into the §2.2 confluent rewrite set unconditionally; `provekit desugar` uses it only under an `--accept-loss` analogue, and the resulting core term carries the loss in its report, the same way the transport artifact does. The exact desugaring is the `loss = ∅` case.

The point: the substrate gets *one* mechanism, "approximate relation, honestly characterized, discharged as the exact check conditioned-or-quotiented", with three instantiations (morphism, composition, desugaring-rewrite), one set of discharge primitives (WPF's Z3 wiring plus `coarsen`), one operational discipline (recorded, not silent; usable only with the precondition established or the loss accepted; the loss visible in the round-trip / output). It is paper 09's lossy-boundary-compression thesis, a boundary that loses information is honest only when the loss is characterized, generalized from the data-boundary case to every place two contract-bearing things meet.

## §7: What it costs, what it's worth, the hard parts

**The work.** The three memento schemas + their three discharge-receipt schemas + the CDDL (§1), small, they are `LanguageMorphismMemento` / `CompositionMemento` / `EquationMemento` plus one formula field each. The generator change (§3): the per-`gap_kind` resolution-option templates, the structured-`reason` emission (mostly moving `diff_reason()`'s output from a sentence into fields), the `PartialMorphismMemento` minting for derivable preconditions, the `transport-gaps.md`-becomes-a-view rewrite. The CLI change (§5): the richer `transport-time:gap` refusal payload, the `--accept-loss` flag and threshold, the `roundtrip_closure: coarsened-by` reporting. The partial / lossy discharge Z3 checks (§1.2, §1.3): the conditioned and quotiented variants of the WPF §3 check plus the `coarsen` formula rewrite, depends on WPF landing (#613); until then the discharge is structural-only and the Z3 path is stubbed. The generalization (§6): the `PartialCompositionMemento` and `LossyDesugaringMemento` shapes, light, mostly "the obvious analogue, wired to the same primitives."

**The payoff.** Gaps stop being notes and become precise, content-addressed, queryable extension requests *with their resolution space attached*, you can ask the catalog "show me every `divergent-semantics:truncated-vs-floored-modulo` gap and what split would close all of them at once." The trichotomy becomes real: a divergence resolves to **exact** (when the contracts coincide), **loudly-bounded-lossy** (the common case for real ports, a `LossyMorphismMemento` with `status: "chosen"`, the loss-set named, the severity tagged, the choice signed, shipped *with* its contract instead of *despite* it), or **refuse** (only when you cannot even characterize the loss). Partial and lossy bridges become *honest*, recorded, conditioned, with their precondition or divergence-set stated, instead of either absent (the gap just sits there, nothing ships) or faked (a relaxation that quietly papers over a real divergence, a silent bug). "We chose to accept this loudly-bounded loss" / "we chose to accept this gap permanently" becomes a signed, content-addressed decision a project can point to, the same kind of artifact as accepting a `pre`, instead of an unwritten understanding. And, the whole point, the substrate's **negative space** (the gaps, the partial bridges, the lossy bridges, the recorded refusals) gets the same rigor and content-addressing as its **positive space** (the exact morphisms, the discharged compositions, the proven contracts). That is "refuse, don't fake; a refusal is a precise extension request" taken to its conclusion: a refusal is not the end of a sentence, it is a memento with a schema, a structured reason, and a menu, and "lossy but loud" is not a confession, it is a first-class result.

**The hard parts, named honestly.**

- *Establishing a partial morphism's precondition at every use-site.* The lift may not carry enough static info, for the dynamic languages (python, ruby, php, javascript surface) it usually will not, so a `PartialMorphismMemento` for `python:add → concept:add` mostly stays a *recorded option the pipeline cannot auto-use*. That is fine. It is still a named, content-addressed bridge with a stated precondition, sitting in the catalog ready for the day the lift learns sort-resolution (WPF §5 already recommends sort-resolved ops for the dynamic languages, which is exactly the lift improvement that would make `operands_statically_int` discharge). The honest statement: this makes the gap *precise and bridgeable in principle*, it does not make the dynamic-language lift smart enough today.
- *The combinatorics of resolution options.* Do not enumerate the universe. The per-`gap_kind` template (§3) lists the *natural* options for each kind, for `divergent-semantics` that is split-the-hub-op plus maybe-a-side-condition-partial-morphism plus accept-permanent, not "every conceivable rewrite." A gap memento with eight resolution options is a sign the template is wrong, not that the gap is hard.
- *The loss-characterization formula language.* It is the WPF formula language, `pre`, `post`, `wp_rule`, loop invariants all live there, and a `loss_characterization` is a formula over the op's formals plus the `disagrees(transported, original)` predicate and the arithmetic-bound predicates (`result_fits_64bit`, `|x| ≥ 2^63`). This ties the spec hard to #613: until `wp_rule` is real, a `loss_characterization` is a formula the substrate can canonicalize and content-address but cannot *discharge against*, the Z3 part of the lossy discharge waits on WPF, the structural-rewrite part (`coarsen` as a syntactic operation) does not.

**Estimated implementation PR count: 6 to 9.** (1) the three memento + three receipt schemas + CDDL + canonicalizer key-order + catalog-directory plumbing; (2) the generator change, structured `reason` emission + per-`gap_kind` resolution templates + `transport-gaps.md`-as-view + `cids.tsv` rows; (3) the `PartialMorphismMemento` minting for derivable preconditions + the structural partial-discharge check; (4) the `LossyMorphismMemento` shape + the `coarsen` formula rewrite + the structural lossy-discharge check + the project mint-config opt-in; (5) the CLI `transport-time:gap` refusal payload + `--accept-loss` + `--accept-loss-threshold` + the `roundtrip_closure: coarsened-by` reporting; (6) the Z3 partial/lossy discharge checks (depends on #613 landing, slips to a follow-up if WPF is not in yet); (7) the generalization, `PartialCompositionMemento` (wired to CCP's Z3) + `LossyDesugaringMemento` (wired to the WPF bi-implication). Realistically Tsavo scopes this as 6-7 if (6) folds into the WPF work and (7) is one PR, ~9 if (6) and (7) are each their own.

## §8: Relationship to the rest

**The morphism / discharge machinery (#609 / #612 / #614).** The exact morphism is the degenerate `precondition = true`, `loss = ∅` case of the partial / lossy ones. PR #612's structural ⊑ discharge that widened coverage 54→91, the `wp-text abstraction` and `pre-weakening` relaxations, is, post-WPF, the WPF §3 refinement check, and a partial morphism is that check with a hypothesis added, a lossy morphism is that check with the target quotiented. Nothing new is invented at the discharge layer; the conditioning and the quotient are the two ways a non-exact relation can still be a relation.

**The wp-as-formula proposal (#613, WPF).** This is load-bearing and worth stating plainly: a `PartialMorphismMemento.validity_precondition` and a `LossyMorphismMemento.loss_characterization` MUST be formulas in the WPF formula language, the same grammar `pre`, `post`, `wp_rule`, loop invariants, and pin invariants use (extended with the arithmetic-bound and `disagrees` predicates), *because that is the only representation you can intersect, compare by `⊆`, propagate through dataflow, and Z3-check against a loss-budget* (§5.1). A severity tag cannot do any of those things; the formula is what makes the gap a negotiation rather than a stop sign. And note the deeper identity: the partial-morphism `precondition`, the lossy-morphism `loss_characterization`, and a contract's `pre` are the *same kind of thing*, a formula carving out the sub-domain where a claim holds, this spec and #613 are one machinery seen from the two sides of the contract (the `wp`/`post` side and the `pre`/domain side), and the loss-propagation of §5.1 is literally `compose`'s `pre`-propagation run on the complementary side. The partial / lossy discharge is a Z3 check just like the exact one (WPF §3.2), conditioned or quotiented. This spec therefore depends on WPF for the Z3 path; without it, the discharge is structural-only and the mementos are content-addressed-but-not-solver-checked, which is still better than a markdown row but is not the end state.

**The desugaring spec (#601, 2026-05-11).** A lossy desugaring is this primitive applied to a rewrite rule (§6): a `LossyDesugaringMemento` is the would-be `DesugaringEquationMemento` plus a `loss_characterization`, discharged as the WPF `wp`-preservation bi-implication quotiented by the loss, admissible into `provekit desugar` only under an `--accept-loss` analogue. The `statement-level-desugaring` resolution option (§2.3) also lives here, lowering an expression-position op with no target ternary is a desugaring move, and the gap memento points at the Desugaring spec for it.

**The PTP (#612).** PTP §3.3's refusal taxonomy gains `transport-time:gap` (§5), replacing the bare `no-morphism-for-op` / `no-target-morphism-for-op` as the standard refusal for a missing exact morphism. PTP §5's round-trip closure gains the `coarsened-by <loss-cid>` and `exact-on-precondition-satisfying-sites` variants (§5). PTP §10's "ops that do not discharge are recorded in `transport-gaps.md`" becomes "are recorded as `TransportGapMemento`s, of which `transport-gaps.md` is a rendered view."

**The "shrink the hub" work (#614 + round 2).** Re-speccing a hub op is the `re-spec-target-op` resolution option: a gap whose `gap_kind` is `arity-shape-mismatch` (the hub op's slot policy is wrong) carries `{ option_kind: "re-spec-target-op", respec_target_to: <the contract the hub op would become>, tradeoff: "..." }`. This spec makes "we considered re-speccing the hub op and chose not to, here's why" recordable, a `re-spec-target-op` option with `status: "rejected"` and the tradeoff as the reason. The hub-shrinking work and the gap catalog are complementary: shrinking the hub closes some gaps (an op the hub no longer demands the wrong shape of), and the ones it does not close get recorded with the re-spec option marked rejected and the reason attached.

**The "refuse, don't fake / a refusal is a precise extension request" principle.** This spec *is* that principle, made into a memento family. The substrate already refused to fake equivalence, it recorded a gap row. This spec makes the gap row a memento: content-addressed, structured-reason, resolution-menu, signable-decision. The negative space stops being a flat list and becomes as rigorous as the positive space. PTP said "a refusal is a precise extension request"; this spec gives the request a schema.

## §9: Why this matters (the closing principle)

The substrate's first axiom *Supra omnia, rectum* binds it, and the substrate has been faithful to it on the positive side: it does not claim a morphism it cannot discharge, it does not call a `wp`-changing rewrite a desugaring, it does not compose two contracts whose post does not imply the next pre. But it has been sloppy on the negative side: when it correctly refuses, the refusal goes into a markdown table, a note, not a memento. A note is not content-addressed. A note carries no structured reason a tool can branch on. A note carries nothing about what one could do instead. A note cannot record "we chose to accept this." The honest design notices that a refusal is *itself* a fact about the substrate, a precise one, with a reason and a resolution space, and gives it the same treatment every other fact gets: a schema, a CID, a signature. After this spec, `python:add → concept:add` is not a row that says "precondition mismatch." It is a `TransportGapMemento` that says: polymorphic arbitrary-precision source op against a fixed-width modular target op; here is a partial morphism valid under `operands_statically_int ∧ result_fits_64bit`; here is a lossy morphism valid if you accept disagreement past 64 bits; here is the option of accepting this permanently, and the project's signed choice, recorded. The negative space, as rigorous as the positive space. That is what the first axiom requires once you take "refuse, don't fake" all the way down: the refusal is not the end of the sentence, it is a memento with a menu.

T Savo
