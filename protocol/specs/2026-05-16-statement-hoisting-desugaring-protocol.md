# Statement-Level Hoisting in the Desugaring Layer

**Version:** v0.1.0 (proposal for review)
**Date:** 2026-05-16
**Status:** design proposal. Sir reacts to this; the implementation is scoped from it.
**Author:** T Savo
**Abbreviation:** SHDP (Statement-Level Hoisting Desugaring Protocol)
**Companion specs:** LSP ([2026-05-09-language-signature-protocol.md](2026-05-09-language-signature-protocol.md)), CCP ([2026-05-09-contract-composition-protocol.md](2026-05-09-contract-composition-protocol.md)), AMP ([2026-05-09-algorithm-memento-protocol.md](2026-05-09-algorithm-memento-protocol.md)), Desugaring and the O(N) Core Compression ([2026-05-11-desugaring-and-the-core-compression.md](2026-05-11-desugaring-and-the-core-compression.md)), Program Transport Protocol ([2026-05-12-program-transport-protocol.md](2026-05-12-program-transport-protocol.md)), `wp` as Formula ([2026-05-13-wp-as-formula.md](2026-05-13-wp-as-formula.md)), Transport Gap and Partial Morphism Protocol ([2026-05-14-transport-gap-and-partial-morphism-protocol.md](2026-05-14-transport-gap-and-partial-morphism-protocol.md)), Concept Hub Abstraction Layer ([2026-05-15-concept-hub-abstraction-layer.md](2026-05-15-concept-hub-abstraction-layer.md))
**Companion papers:** [paper 07: After Verification](../../docs/papers/07-after-verification-bug-classes-as-missing-edges.md), [paper 13: After Grammars](../../docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md), [paper 16: After Portability](../../docs/papers/16-after-portability-the-universal-address-space.md), [paper 17: After Babel](../../docs/papers/17-after-babel-we-speak-in-vectors-now.md)

## §0: Why this spec exists

The desugaring layer (2026-05-11) rewrites source terms into concept-hub terms by emitting `DesugaringEquationMemento` records. Each memento is a flat equation: `lhs = rhs`, both sides at the same term position. The lhs is the source op at expression position; the rhs is the concept-hub op that replaces it in place. The rewrite is position-preserving: the parent node still has a child at the same slot; the child's CID changes.

That shape covers the common case. It does not cover the class of ops that, when realized into a target language, have no expression-position form at all.

Go is the canonical instance. `&&` and `||` short-circuit correctly in Go, but there is no expression that evaluates both operands and selects one; the only way to express short-circuit conjunction in Go is with an `if` block. That block is a statement. When the transport layer realizes `concept:and` into Go, the surrounding term cannot simply receive a replacement expression at the child slot. The realization must: (1) introduce a fresh temp variable scoped to the enclosing statement block, (2) emit a sequence of statements initializing that temp conditionally, and (3) substitute `var(temp)` at the original child slot.

This is statement-level hoisting. The surrounding term's structure has to accept the temp binding. The flat `lhs = rhs` shape cannot record this: there is no field for the statement prelude, no slot for the fresh name, no loss characterization for the scope-namespace extension. And the `wp`-preservation obligation cannot be discharged against the existing memento type because the two things being equated are not at the same term position.

This spec defines `HoistingDesugaringMemento`, a new memento type that refines `DesugaringEquationMemento` in the LSP §4.4 successor-mint sense: every flat desugaring is a hoist with an empty prelude (the degenerate case), so the existing type is a structural subtype of the new one. The new type adds the `prelude` field (a sequence of statements), the `fresh_name_slot` (the naming convention for the introduced temp), a `target_lang` field, and the loss-record and discharge-receipt fields required by *Supra omnia, rectum*.

This is a proposal, the same shape as the three sibling specs (#613, #616, #617). It defines the schema, the composition rule, the `wp`-preservation obligation, and the loss-record characterization, in enough detail to scope the implementation from.

### §0.1 The premise, stated plainly

**Hoisting is not a hack.** It is the principled realization of a concept op into a target that lacks an expression form of it. The 2026-05-15 spec establishes that every general-purpose target can realize every concept, possibly lossily. Hoisting is the mechanism for the expression-position case: when the target's realization is a statement-level pattern, the transport emits a prelude of statements and a fresh temp, the fresh temp is the expression-position substitute, and the loss is precisely characterized. The "not a hack" claim is grounded in `wp`-preservation: the hoist is correct iff `wp(<source expression>, Q) = wp(<prelude; var(temp)>, Q)`, and this spec states that as a dischargeable obligation.

**Hoisting is always loudly-bounded-lossy in the trichotomy sense.** Every hoist introduces at least one fresh name into the target's scope. Fresh names are not in the source. That introduction is a `structural_divergence` loss: the target has a scope-namespace the source does not. The trichotomy (exact / loudly-bounded-lossy / refuse) places all hoists in the second bucket, not the first. That is the honest position: hoisting ships something useful AND names the divergence precisely. Silent hoisting, a rewrite that introduces a temp without recording the scope-extension in a loss record, is what the substrate refuses.

## §1: The memento type

### §1.1 The shape

`HoistingDesugaringMemento` records a single hoist: one expression-position op being realized into a temp binding plus a statement prelude.

```cddl
; Imports:
;   ir-formula    ; from 2026-04-30-ir-formal-grammar.md, extended per WPF §2.3 and TGP §1.3
;   cid           ; content-addressed identifier, per LSP §1.4

hoisting-desugaring-memento = {
  schema_version:  "1",
  kind:            "HoistingDesugaringMemento",
  fn_name:         tstr,               ; e.g. "hoist:concept:and:to:go"

  ; The source side.
  lhs:             ir-term,            ; the op at expression position being hoisted; e.g. concept:and(a, b)
  lhs_cid:         cid,               ; CID of lhs's contract (the source op's operation-contract)

  ; The target side, split into prelude + expression substitute.
  prelude:         [+ ir-stmt],        ; one or more target-language statements to emit BEFORE the surrounding stmt
                                       ; each ir-stmt is over the target language's core ops
                                       ; the sequence is ordered: emit prelude[0], then prelude[1], ..., then continue
  rhs:             ir-term,           ; the expression-position substitute; almost always var(fresh_name_slot)
  rhs_cid:         cid,              ; CID of rhs's contract (the temp-read operation-contract)

  ; The fresh name.
  fresh_name_slot: tstr,              ; the name used for the introduced temp across prelude and rhs
                                       ; convention: "__pk_hoist_<n>__" where n is the deterministic AST traversal counter
                                       ; n is stable under JCS-canonical bytes of the enclosing lift context

  ; Language.
  target_lang:     tstr,              ; e.g. "go", "zig", "c11"

  ; Loss characterization (per TGP §1.3, the multidimensional loss-record schema).
  loss_record:     loss-record,

  ; wp-preservation discharge.
  discharge_receipt: hoisting-discharge-receipt / null,  ; null = not yet discharged

  ? signature:     tstr / null
}

loss-record = {
  * loss-dimension => ir-formula
}

; The degenerate case: flat desugaring is a hoist with empty prelude and empty loss.
; DesugaringEquationMemento refines to HoistingDesugaringMemento with
;   prelude = [],  rhs = <original rhs>,  fresh_name_slot = "",  loss_record = {}
; per LSP §4.4 successor-mint conventions.
```

### §1.2 The discharge receipt

```cddl
hoisting-discharge-receipt = {
  schema_version:  "1",
  kind:            "HoistingDischargeReceipt",
  memento_cid:     cid,               ; the HoistingDesugaringMemento being discharged
  lhs_cid:         cid,
  rhs_cid:         cid,
  obligation:      "wp-hoist-equivalence",
  method:          tstr,              ; e.g. "z3-portfolio", "structural-match", "manual-proof"
  discharged:      bool,
  ? witness:       any,               ; Z3 model or proof term, per WPF §3
  ? loss_budget_cid: cid,            ; if a loss-budget authorized this hoist's loss_record per TGP §5.1
  ? signature:     tstr / null
}
```

### §1.3 The naming convention for fresh temps

The fresh name for a hoist is `__pk_hoist_<n>__` where `n` is the count of hoist applications in the enclosing lift context, incremented in depth-first AST traversal order over the JCS-canonical bytes of the source term. Determinism properties: (a) given the same source term, the same n is assigned to the same op regardless of implementation language; (b) hoist names in a composed expression are assigned in source-order (leftmost-first, see §2); (c) the counter resets per function body, not per file.

The convention has two purposes. First, it keeps the fresh names out of the user's namespace: a prefix + suffix double-underscore is conventionally reserved in most languages (C, C++, Go, Zig all treat `__`-bounded names as implementation-reserved). Second, it makes the names stable, so the CID of a `HoistingDesugaringMemento` does not depend on incidental counter state.

## §2: The composition rule

When a source expression contains multiple ops that each require a hoist, the hoists compose in source-order. This section states the rule normatively.

**Setting.** A source expression E at statement position P contains K ops each requiring a hoist to the target language. Let those ops be `o_1, o_2, ..., o_K`, enumerated in depth-first left-to-right AST traversal order of E. Each `o_i` produces a hoist with fresh name `t_i = __pk_hoist_<i>__` (with i starting at the base counter for P, incremented per hoist), prelude `pre_i`, and rhs `var(t_i)`.

**The rule.**

1. The composed prelude is `pre_1; pre_2; ...; pre_K`, in that order. No permutation is allowed: source-order is the only stable order, and reordering preludes may change observable behavior when any `pre_i` has effects (see §4 on `effect_divergence`).
2. The composed rhs is the original expression E with each `o_i` replaced by `var(t_i)`, in source-order. The result is an expression over target-language primitives.
3. The fresh-name counters are stable: `t_1` through `t_K` are the names, regardless of how the K hoists were individually generated.
4. Each `o_i`'s `HoistingDesugaringMemento` is a separate memento in the pool. The composed result is their sequential application, NOT a single merged memento. The pool retains each individual fact.

**Example.** The expression `(a && b) || (c && d)` lowered to Go:

- `o_1 = and(a, b)`, fresh name `__pk_hoist_0__`, prelude emits the if-block for the first `&&`.
- `o_2 = and(c, d)`, fresh name `__pk_hoist_1__`, prelude emits the if-block for the second `&&`.
- `o_3 = or(__pk_hoist_0__, __pk_hoist_1__)`, fresh name `__pk_hoist_2__`, prelude emits the if-block for `||`.

The composed prelude is `<pre_1>; <pre_2>; <pre_3>`. The composed rhs is `var(__pk_hoist_2__)`. The three mementos are separate. The overall wp obligation is the conjunction of the three individual obligations plus the sequential composition rule for preludes (§3).

**Why source-order is mandatory.** Two hoists at the same level in an expression may have effects: `concept:and` hoists emit reads of `b` conditionally (the McCarthy short-circuit); those reads may have effects. Reordering `pre_1` and `pre_2` may execute `b` before `a` has been read, or execute `d` before `c`. That is a real behavioral difference. Source-order is the only ordering that matches the source program's evaluation order. Any other ordering is a value_divergence or effect_divergence, not a structural one, and must be recorded explicitly.

## §3: The wp-preservation obligation

A hoist is correct iff it preserves the weakest precondition of the source op. This section states the obligation as a dischargeable refinement check, in the style of WPF §3.

**The obligation.** Let `lhs = concept:and(a, b)` be the source expression at expression position `e` inside statement position `P(e)` (the surrounding statement with a hole at `e`). Let `prelude` be the statement sequence the hoist emits, and let `rhs = var(t)` be the expression-position substitute. The hoist is wp-preserving iff:

```
forall Q.  wp(P(lhs), Q)  =  wp(prelude; P(var(t)), Q)
```

where `wp` is evaluated by the WPF §2 evaluator.

**Reduction to sequential composition.** The sequential composition rule for `wp` gives:

```
wp(prelude; P(var(t)), Q)  =  wp(prelude, wp(P(var(t)), Q))
```

So the obligation reduces to: the prelude's weakest precondition, post-composed with the `wp` of the rhs in context, equals the source expression's `wp` in context. For a single-statement prelude `if !a { t = false } else { t = b }` (the `concept:and → go` hoist), this unfolds to a standard conditional wp:

```
wp(if !a { t = false } else { t = b }, wp(P(var(t)), Q))
=  (!a ==> wp(t = false, wp(P(var(t)), Q)))
&& (a  ==> wp(t = b,    wp(P(var(t)), Q)))
=  (!a ==> Q[t := false])
&& (a  ==> Q[t := b])
```

This must equal `wp(concept:and(a, b), Q)`, which by the WPF hub rule for `concept:and` is the McCarthy rule: `!a ==> false satisfies Q; a ==> b satisfies Q`, i.e., `Q[t := (a && b)]`. For the case where Q mentions t only through `var(t)`, these coincide: the obligation discharges.

**Discharge method.** The WPF §3 evaluator evaluates `wp(lhs, Q)` and `wp(prelude; var(t), Q)` symbolically, then hands the biconditional to the Z3 portfolio. The receipt carries `obligation: "wp-hoist-equivalence"`, `method: "z3-portfolio"`, and the witness from Z3. For cases where structural matching suffices (the prelude's wp unfolds to the McCarthy/Dijkstra rule by rewriting alone, without solver calls), `method: "structural-match"` is recorded.

**Composed hoists.** For K hoists composed in source-order (§2), the obligation is:

```
forall Q.  wp(P(E), Q)  =  wp(pre_1; pre_2; ...; pre_K; P(rhs), Q)
```

This decomposes by sequential wp composition into K nested obligations, each matching one hoist's individual discharge. The composed receipt records all K individual receipts by CID and asserts the sequential conjunction.

## §4: The loss-record characterization

Every `HoistingDesugaringMemento` carries a `loss_record` over the multidimensional schema of TGP §1.3. This section characterizes the two dimensions that hoisting always touches.

### §4.1 `structural_divergence`

A hoist always introduces a fresh name into the target's scope. The source program has no such name. This is a structural divergence: the target's scope-namespace has a member the source does not.

The `structural_divergence` formula for a hoist is:

```
structural_divergence = { "scope-namespace": "fresh var " + fresh_name_slot + " introduced at enclosing statement boundary" }
```

The 2026-05-15 spec minted `structural_divergence` at the abstraction tier. Hoisting at the op tier is the same dimension: a realization whose surface form does not resemble the source. The dimension is reused, not re-minted. The TGP §1.3 schema is open (`/ tstr`) and the dimension is the same named string.

`structural_divergence` is never empty for a hoist. A hoist with no fresh-name introduction is a flat desugaring, represented by the degenerate form (`prelude = []`), which is `DesugaringEquationMemento`. A non-degenerate hoist has `structural_divergence` non-empty by construction.

Severity: `"rare-in-practice"` for most source programs (the temp is implementation-internal and not user-visible), but `"common"` in contexts where the target's scope or namespace analysis is precision-sensitive (e.g., name-shadowing checks in Zig, where `__pk_hoist_*__` is valid but visible to the compiler's shadow linter).

### §4.2 `effect_divergence`

`effect_divergence` is non-empty when the hoisted op's right-hand operand has effects and the hoist serializes those effects in a different order than the source. For pure expressions, `effect_divergence = ∅`. For impure expressions, `effect_divergence` records the ordering commitment.

Example: `concept:and(f(), g())` where `f` and `g` have effects. The source's `wp` rule for `concept:and` is McCarthy: `g()` is called only if `f()` returns true, in source order. The hoist preserves this: the prelude is `if !f() { t = false } else { t = g() }`. The call to `g()` is inside the else branch, preserving the short-circuit. So `effect_divergence = ∅` here, because the serialization matches.

The case where `effect_divergence` is non-empty: a hypothetical hoist that evaluates both operands eagerly before branching (a wrong hoist) would diverge. The SHDP hoists do not do this. The wp-preservation obligation (§3) ensures it: an eager-evaluation hoist would fail the Z3 discharge.

The practical rule: a correctly discharged hoist has `effect_divergence = ∅` whenever the wp-preservation obligation holds, because wp-preservation is precisely the absence of observable behavioral difference including effect ordering. A hoist that cannot discharge wp-preservation may carry a non-empty `effect_divergence`; that is then a `LossyMorphismMemento` territory (TGP §1.4), not SHDP territory, because the hoist is making a deliberate approximation.

### §4.3 `value_divergence` and `ub_introduction`

For the `concept:and → go` and `concept:ite → go` worked examples (§5, §6), and for all correctly discharged hoists, `value_divergence = ∅` and `ub_introduction = ∅`. Hoisting introduces a fresh name and a statement prelude; it does not change the set of input states on which the result is defined or the value the result takes on those states. If a hoist changed the value, it would not discharge the wp-preservation obligation of §3, and the substrate would refuse to mint the memento.

## §5: Worked example: `concept:and → go`

### §5.1 The setting

Go has no expression-form short-circuit conjunction. `&&` exists as an expression but it is a statement-equivalent in a language without ternary: to use the result of a conjunction as a value in a larger expression, one must hoist. The transport of `concept:and(a, b)` to Go therefore always produces a hoist.

Source term: `concept:and(a, b)` at expression position `e` inside a statement `P(e)`.

### §5.2 The memento

```json
{
  "schema_version": "1",
  "kind": "HoistingDesugaringMemento",
  "fn_name": "hoist:concept:and:to:go",
  "lhs": { "op": "concept:and", "args": ["a", "b"] },
  "prelude": [
    {
      "op": "go:var-decl",
      "name": "__pk_hoist_0__",
      "type": "bool",
      "value": { "op": "go:false-lit" }
    },
    {
      "op": "go:if",
      "cond": "a",
      "then": [
        {
          "op": "go:assign",
          "lhs": "__pk_hoist_0__",
          "rhs": "b"
        }
      ],
      "else": []
    }
  ],
  "rhs": { "op": "go:var-read", "name": "__pk_hoist_0__" },
  "fresh_name_slot": "__pk_hoist_0__",
  "target_lang": "go",
  "loss_record": {
    "structural_divergence": "fresh var __pk_hoist_0__ introduced at enclosing statement boundary"
  },
  "discharge_receipt": null
}
```

The prelude initializes `__pk_hoist_0__` to `false`, then if `a` is true assigns `b` to it. The rhs reads `__pk_hoist_0__`.

### §5.3 The wp check

```
wp(concept:and(a, b), Q)
  = McCarthy rule: if a then wp(b suffices, Q) else wp(false, Q)
  = (a ==> Q[result := b]) && (!a ==> Q[result := false])

wp(prelude; var(__pk_hoist_0__), Q)
  = wp(var __pk_hoist_0__ = false; if a { __pk_hoist_0__ = b }, Q[result := __pk_hoist_0__])
  = wp(var __pk_hoist_0__ = false, wp(if a { __pk_hoist_0__ = b }, Q[result := __pk_hoist_0__]))
  = wp(var __pk_hoist_0__ = false,
       (a ==> Q[result := __pk_hoist_0__][__pk_hoist_0__ := b])
    && (!a ==> Q[result := __pk_hoist_0__]))
  = (a ==> Q[result := b]) && (!a ==> Q[result := false])
```

These are equal. Discharge is `structural-match`. `value_divergence = ∅`, `effect_divergence = ∅`.

### §5.4 The loss summary

The `concept:and → go` hoist is loudly-bounded-lossy with exactly one loss dimension: `structural_divergence: scope-namespace`. The McCarthy semantics is preserved exactly. The result value on all inputs is identical to the source. No undefined behavior is introduced. The only thing the target has that the source does not is the variable `__pk_hoist_0__` in the enclosing scope, and that is precisely what the `structural_divergence` record names.

## §6: Worked example: `concept:ite → go`

### §6.1 The setting

Go has no ternary expression. `if ... else ...` is a statement. To use an if-else result as a value in a larger expression, one must hoist. The transport of `concept:ite(cond, then_expr, else_expr)` to Go therefore always produces a hoist.

Source term: `concept:ite(cond, then_expr, else_expr)` at expression position `e` inside a statement `P(e)`.

### §6.2 The memento

```json
{
  "schema_version": "1",
  "kind": "HoistingDesugaringMemento",
  "fn_name": "hoist:concept:ite:to:go",
  "lhs": { "op": "concept:ite", "args": ["cond", "then_expr", "else_expr"] },
  "prelude": [
    {
      "op": "go:var-decl",
      "name": "__pk_hoist_0__",
      "type": "<result-type>",
      "value": { "op": "go:zero-value" }
    },
    {
      "op": "go:if",
      "cond": "cond",
      "then": [
        {
          "op": "go:assign",
          "lhs": "__pk_hoist_0__",
          "rhs": "then_expr"
        }
      ],
      "else": [
        {
          "op": "go:assign",
          "lhs": "__pk_hoist_0__",
          "rhs": "else_expr"
        }
      ]
    }
  ],
  "rhs": { "op": "go:var-read", "name": "__pk_hoist_0__" },
  "fresh_name_slot": "__pk_hoist_0__",
  "target_lang": "go",
  "loss_record": {
    "structural_divergence": "fresh var __pk_hoist_0__ introduced at enclosing statement boundary"
  },
  "discharge_receipt": null
}
```

The prelude declares `__pk_hoist_0__` with the result type's zero value, then assigns `then_expr` or `else_expr` into it depending on `cond`. The rhs reads `__pk_hoist_0__`.

`<result-type>` is a placeholder for the concrete type of `then_expr`/`else_expr`; the lift context carries the type from the sort-resolution pass. The zero value initialization is the Go idiom and is value-neutral: it is overwritten before the rhs is read.

### §6.3 The wp check

```
wp(concept:ite(cond, then_expr, else_expr), Q)
  = (cond ==> wp(then_expr, Q)) && (!cond ==> wp(else_expr, Q))

wp(prelude; var(__pk_hoist_0__), Q)
  = wp(var __pk_hoist_0__ = zero; if cond { __pk_hoist_0__ = then_expr } else { __pk_hoist_0__ = else_expr },
       Q[result := __pk_hoist_0__])
  = (cond ==> Q[result := then_expr]) && (!cond ==> Q[result := else_expr])
```

These are equal under the standard conditional wp rule. Discharge is `structural-match`. `value_divergence = ∅`, `effect_divergence = ∅`. Same loss as §5.4: `structural_divergence: scope-namespace` only.

### §6.4 Distinction from `concept:and → go`

Both hoists have the same loss record and the same discharge method. The difference is in the prelude: `concept:ite` always evaluates exactly one branch (the semantics demands it, and the go:if respects it), while `concept:and` short-circuits by evaluating the rhs only if lhs is true. The `concept:and` hoist's prelude does not evaluate `b` in the else branch; the `concept:ite` hoist's prelude evaluates exactly one of `then_expr` / `else_expr`. Both preserve McCarthy semantics for the source concept. Both discharge at `structural-match`.

## §7: Relationship to the rest

**The Desugaring spec (2026-05-11).** `HoistingDesugaringMemento` is a successor to `DesugaringEquationMemento` in the LSP §4.4 sense, with `refines = <2026-05-11 DesugaringEquationMemento CID>`. The flat equation is the `prelude = []` degenerate case. The 2026-05-11 soundness condition ("a desugaring preserves `wp`") becomes the §3 obligation here, stated at statement level. The two specs are not in conflict; SHDP extends the desugaring vocabulary for the statement-level case.

**WPF (2026-05-13, #613).** The discharge obligation of §3 is a direct application of WPF §3.2: the same refinement check, the same Z3 portfolio, the same `∀Q` structure. The SHDP `wp`-hoist-equivalence obligation is the WPF §3 biconditional check applied to `wp(lhs, Q)` vs. `wp(prelude; rhs, Q)`. No new evaluator infrastructure is needed; SHDP receipts are one more kind of WPF discharge receipt.

**TGP (2026-05-14, #616).** The `loss_record` schema is TGP §1.3's, reused without modification. The `structural_divergence` dimension was minted at the abstraction tier in the 2026-05-15 spec; SHDP reuses it at the op tier (the schema is open per TGP §1.3: `/ tstr`). The TGP trichotomy (exact / loudly-bounded-lossy / refuse) places all non-degenerate hoists in the second bucket: they are loudly-bounded-lossy by construction. The loss-budget gate of TGP §5.1 applies: `provekit transport` will not emit a hoist unless the migration's loss-budget admits `structural_divergence: scope-namespace` at the declared severity.

**Concept Hub spec (2026-05-15, #617).** The two worked examples (§5, §6) are realizations at the operation layer, not the abstraction layer. They sit below the abstraction tier: `concept:and` and `concept:ite` are operation-layer hub nodes (§1.1 of that spec). SHDP is what the 2026-05-15 §0.1 premise relies on when it says "Go has no ternary" is not a refusal: hoisting is the mechanism that makes the claim good. The abstraction-layer realizations of the 2026-05-15 spec (e.g., `concept:dynamic-dispatch → go:interface-value`) may themselves produce operation-layer terms that require hoisting; those hoists are SHDP instances.

**PTP (2026-05-12).** `provekit transport` emits `HoistingDesugaringMemento` records during the realization pass when the target language lacks an expression form. The transport CLI refusal of PTP §3 is extended: a gap whose only resolution is a hoist produces a `HoistingDesugaringMemento` (with `discharge_receipt: null`) and a `LossyMorphismMemento` naming `structural_divergence: scope-namespace`, not a bare refusal. A bare refusal is reserved for cases where no characterized hoist is possible.

**LSP (2026-05-09).** The naming convention of §1.3 (the deterministic counter from JCS-canonical bytes) is an LSP §3 fact: it participates in the memento's CID. Two implementations that agree on the lift context's JCS-canonical bytes must produce the same fresh name for the same hoist site. That is the only cross-language coordination point SHDP adds on top of the existing LSP requirements.

## §8: What it costs, what it is worth, the hard parts

**The work.** The new schema: small, it is one new memento type, one new receipt type, two new CDDL productions, and a successor-mint registration per LSP §4.4. The generator change: the per-target realization-desugaring emitter gains a hoist-detection pass (does this concept op have an expression-form in the target? if not, emit `HoistingDesugaringMemento`). The WPF discharge wiring: the §3 obligation is a standard WPF biconditional check, the evaluator already exists (#613 when landed), wiring is one new obligation kind. The fresh-name counter: a single integer threaded through the lift context, reset per function body. The loss-budget integration: `structural_divergence: scope-namespace` needs a severity tag in the budget schema (TGP §5.1), which is a one-line catalog addition.

**The payoff.** The gap between "concept op exists" and "target language can receive it" closes for the expression-position-missing case. `concept:and → go`, `concept:ite → go`, `concept:or → go`, `concept:ite → zig` (no ternary in Zig's type-checked context), and the analogous cases in other targets all become loudly-bounded-lossy hoist mementos rather than unmarked transport gaps. The downstream consequence: the 2026-05-15 premise ("every admissible target can realize every concept") holds mechanically, not just in principle, for these op-layer cases. The transport pipeline stops deferring on "no expression form" and starts emitting characterized, discharged, signed artifacts.

**The hard parts, named honestly.**

- *Type inference for `<result-type>`.* The `concept:ite → go` hoist needs the result type to declare the zero-value initialization. The lift context must carry sort information from the type-inference pass. For well-typed source programs this is available; for ill-typed or partially-typed programs (dynamic language origins) it may not be. In the latter case the hoist falls back to `interface{}` (Go's any type), which introduces an additional `structural_divergence: type-erasure` entry in the loss record.
- *Nested hoists at the same statement level.* The composition rule of §2 handles this by source-order counter assignment. The hard case is when two hoists interact through shared temporaries or when the enclosing statement is itself a hoist rhs. The solution: the prelude sequences are always emitted in source-order and each hoist's fresh name is unique within the function body by construction. The wp obligation for composed hoists (§3, last paragraph) is decomposable into K individual obligations; the hard part is that K can be large for deeply nested expressions. This is a discharge-cost issue, not a correctness issue: the portfolio handles each sub-obligation independently.
- *Scope capture.* In Go, a variable declared in a block is scoped to that block. If the hoist is inside a loop body or a conditional arm, `__pk_hoist_<n>__` must be declared in the enclosing function scope (not the block) to be readable by the rhs at the expression position. The prelude emission must track the enclosing statement boundary, not just the enclosing block boundary. This is an implementation concern, not a schema concern, but the spec must note it: the `prelude` field's ir-stmts are emitted at the *function-scope-nearest* enclosing statement boundary, not the nearest block boundary.

## §9: What this proposal does and does not claim

This proposal defines the `HoistingDesugaringMemento` schema, the composition rule, the wp-preservation obligation, and the loss-record characterization. It does not:

- Implement the hoist emitter in any language's lifter or transport layer.
- Provide a catalog of all ops requiring hoisting per target language (that is a catalog spec, separate, scoped from this).
- Discharge the worked examples (§5, §6): the receipts are `null` pending WPF (#613) landing and the Z3 wiring.
- Define the scope-tracking rules for loop-body hoists in detail: that is an implementation spec for the per-language transport plugin.

The proposal is a schema spec, a composition rule, and a wp-obligation statement. It is the same shape and scope as the sibling specs (#613, #616, #617).

## §10: Why this matters (the closing principle)

The desugaring layer's promise, from the 2026-05-11 spec, is that the N-op language core compresses to the `concept:*` hub in O(N) work. That compression assumes every op has a realization path into the hub. Hoisting is the path for the class of ops whose realization requires a statement-level introduction in the target. Without it, those ops sit as unmarked transport gaps, and the compression is incomplete for real programs.

*Supra omnia, rectum.* The substrate does not paper over the gap with a silent rewrite. It records the hoist, names the loss, discharges the wp obligation, and ships the artifact with its contract attached. A hoist with `structural_divergence: scope-namespace` and a discharged receipt is a more honest artifact than a hand-written desugaring with no contract at all, which is what every compiler in existence ships today.

The closing claim: a transport pipeline that emits characterized, discharged `HoistingDesugaringMemento` records for every expression-position gap is a pipeline that can be *audited*. An auditor reading the memento pool knows exactly which ops required a statement-level introduction, what the introduced names are, that the wp is preserved, and what the loss is. That is what it means to prove `k(I) = t`.
