# Weakest-Precondition Rules as Machine-Checkable Formulas

**Version:** v0.1.0 (draft)
**Date:** 2026-05-13
**Status:** design draft for review
**Author:** T Savo
**Companion specs:** LSP (2026-05-09-language-signature-protocol.md), CCP (2026-05-09-contract-composition-protocol.md), AMP (2026-05-09-algorithm-memento-protocol.md), Desugaring and the Core Compression (2026-05-11-desugaring-and-the-core-compression.md), PTP (2026-05-12-program-transport-protocol.md), LoopInvariantMemento (2026-05-05-loop-invariant-memento.md), IR Formal Grammar (2026-04-30-ir-formal-grammar.md)
**Companion papers:** [paper 07: After Verification](../../docs/papers/07-after-verification-bug-classes-as-missing-edges.md), [paper 13: After Grammars](../../docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md), [paper 17: After Babel](../../docs/papers/17-after-babel-we-speak-in-vectors-now.md)

## §0: Why this spec exists

Every operation contract today carries a `wp` field that is a prose string. `op_add.spec.json` says `"wp": "mathematical integer addition when no overflow holds"`. `op_div.spec.json` says `"wp": "integer division expression"`. The Zig `bop_div` op says `"wp": "Zig binary / over modeled operands"`. The `op_if.spec.json` op is the one outlier that already writes the Dijkstra rule in pseudo-syntax: `"wp": "cond ? wp(then_branch, post) : wp(else_branch, post)"`. None of these are reasoned over. They are documentation glued onto the canonical bytes of a contract.

That is the single highest-leverage gap in the substrate right now, for one reason: the contract triple is `(pre, post, implications)` and `wp` is the part that turns the triple into a *function*. Without a machine-checkable `wp` you cannot compute `wp(op, Q)` (the predicate transformer), you cannot compose two contracts correctly (CCP composes `pre`/`post` but has no way to push a postcondition backward through an op whose only `wp` is a sentence in English), and you cannot prove refinement (`⊑`). The cross-language transport layer (PTP §2.3, §2.4) currently discharges a morphism `<lang>:op → concept:op` by canonicalizer-equality, and when that fails, by two named structural relaxations: `wp-text abstraction` ("specs differ only in `post.wp`, ignore it") and `pre-weakening` ("`lang_pre = true`, so ignore `pre` too"). The `mint_language_morphisms.py` header documents both. Roughly half of the unclosed `transport-gaps.md` rows are prose-`wp` mismatches: two ops that *are* the same operation, recorded as not-the-same, because the only thing distinguishing their contracts is one English sentence versus a different English sentence.

The fix is already most of the way built. Sugar has a structured formula language: `pre` and `post` are term trees in the IR formula grammar (`{kind:"atomic",...}`, `{kind:"op",...}`, `{kind:"var",...}`, `{kind:"const",...}`, plus the boolean connectives `and`/`or`/`not`/`implies` and quantifiers). For a pure value-op, `wp(op, Q)` is just `op.pre ∧ Q[result_value := value_expr]`, where `value_expr` is read off `post` (which already has the shape `result_value == value_expr`). For a control-flow op it is the standard Dijkstra rule. And there is already a `wp.rs` in the walk-c lifter (compose.rs carries the comment "duplicated from walk's wp.rs"): the *outer* structural recursion over a function body exists. What is missing is the *inner* rule, per op, as data instead of as hardcoded match-arms in each lifter. This spec proposes carrying that rule in the operation contract, in the existing formula language, content-addressed like everything else, and rebuilding the transport discharge on top of it as a real refinement check.

This is paper 17 §5 made concrete: `compose = wp, federated`. It is paper 13's homomorphism obligation made precise: a morphism preserves `wp` is *the* statement of "preserves the contract." And it is the realization of the paper-17 thesis that the `wp` rule *is* the Curry-Howard proof of the operation's contract theory, the proof-as-program for the op, addressed by its CID like every other proof in the substrate.

This is a design draft for review, not a decision. It defines the schema, the evaluator, the discharge rework, and the migration path concretely enough to scope from.

## §1: The `wp`-rule schema

### §1.1 The shape

An operation contract gains one field, `wp_rule`, a sibling of `pre` and `post`. The existing prose `wp` is renamed `wp_note`, kept, marked optional and non-load-bearing (UI and documentation only), and excluded from any obligation. This is a coexistence arc, not a hard cutover: the canonicalizer accepts a contract with `wp_note` and no `wp_rule` during the migration window (§4); past the window, an operation contract without a `wp_rule` is a non-load-bearing contract that refuses downstream composition, exactly the way an op carrying an `opaque_loop` effect refuses it today.

A `wp_rule` is a term in the *existing* IR formula language (the same grammar that `pre`, `post`, loop invariants, and pin invariants use), parametric in two reserved meta-variables:

- **`Q`** — a free variable of formula sort, standing for the postcondition the rule is applied to.
- **`wp_<slot_name>`** — for each `Stmt`-sorted slot of the op, a free function-typed meta-variable standing for "the `wp` transformer of the term plugged into that slot." A `wp_<slot>` symbol is applied to one formula argument and yields a formula: `wp_then_branch(Q)` is "the weakest precondition of the then-branch term with respect to `Q`."

Substitution of `Q` and instantiation of `wp_<slot>` is the *same* `var`-substitution machinery that `pre`/`post` already use; nothing new is introduced. The `wp_rule` is JCS-canonical, hashed with BLAKE3-512, and part of the operation contract's canonical bytes (so two ops with different `wp_rule`s are different ops, which is correct).

### §1.2 The pure-value-op derivation (why this is not 80 rules per language)

For an op all of whose slots are value-sorted (`Int`, `Bool`, a user sort, anything not `Stmt`), there are no `wp_<slot>` meta-variables, and the rule is forced by `pre` and `post`:

```
wp_rule(Q)  ≡  pre  ∧  Q[result_value := value_expr]
```

where `value_expr` is the right-hand side of `post`, which for a value-op always has the form `result_value == value_expr` (or `result_value = value_expr`). This is derivable from `pre` and `post` by inspection. Therefore `wp_rule` is **REQUIRED only on `Stmt`-sorted ops** (`if`, `while`, `seq`, `return`, `break`, `continue`, `try`/`catch`/`finally`, `throw`, `call`, `assign`, the loop forms). For every value-op, `wp_rule` is OPTIONAL: when absent, the substrate synthesizes it from `pre`/`post` via the formula above, and a value-op MAY carry `wp_rule` explicitly only when its synthesized form would be wrong (it never should be; the synthesis is total over well-formed value-op contracts). That cuts the authored-rule count from ~80 ops per language to roughly 10 to 20 control-flow ops per language.

### §1.3 Worked examples

**`concept:add`** (value-op; `pre = no_signed_overflow(add(lhs, rhs))`, `post = (result_value == add(lhs, rhs))`). Synthesized, not authored:

```
wp_rule(Q)  ≡  no_signed_overflow(add(lhs, rhs))  ∧  Q[result_value := add(lhs, rhs)]
```

In IR-formula JSON (the `and` connective from the grammar, `Q` a `var`):

```json
{
  "kind": "and",
  "operands": [
    { "kind": "atomic", "name": "no_signed_overflow",
      "args": [ { "kind": "op", "name": "add",
        "args": [ {"kind":"var","name":"lhs"}, {"kind":"var","name":"rhs"} ] } ] },
    { "kind": "substitute",
      "target": { "kind": "var", "name": "Q" },
      "var": "result_value",
      "term": { "kind": "op", "name": "add",
        "args": [ {"kind":"var","name":"lhs"}, {"kind":"var","name":"rhs"} ] } }
  ]
}
```

(`substitute` is a new formula node — an explicit, capture-avoiding, single-variable substitution on a formula — discussed in §2.3. It is the one grammar addition this spec needs.)

**`concept:div`** (value-op; `pre = not_zero(rhs)`, `post = (result_value == div(lhs, rhs))`). Synthesized:

```
wp_rule(Q)  ≡  not_zero(rhs)  ∧  Q[result_value := div(lhs, rhs)]
```

**`concept:conditional`** / `concept:if` (control-flow op; slots `cond` value-sorted, `then_branch` and `else_branch` `Stmt`-sorted). Authored, the Dijkstra rule, parametric in the two slot transformers:

```
wp_rule(Q)  ≡  (cond ⇒ wp_then_branch(Q))  ∧  (¬cond ⇒ wp_else_branch(Q))
```

```json
{
  "kind": "and",
  "operands": [
    { "kind": "implies", "operands": [
      { "kind": "var", "name": "cond" },
      { "kind": "apply", "fn": "wp_then_branch", "args": [ {"kind":"var","name":"Q"} ] } ] },
    { "kind": "implies", "operands": [
      { "kind": "not", "operands": [ {"kind":"var","name":"cond"} ] },
      { "kind": "apply", "fn": "wp_else_branch", "args": [ {"kind":"var","name":"Q"} ] } ] }
  ]
}
```

(`apply` of a `wp_<slot>` meta-var is the second grammar addition — see §2.3.)

**`concept:seq`** (control-flow op; slots `first` and `second` both `Stmt`-sorted). Authored:

```
wp_rule(Q)  ≡  wp_first(wp_second(Q))
```

This is exactly the `op_seq.spec.json` pseudo-syntax `wp(first, wp(second, post))`, now a term.

**`concept:while`** (control-flow op; slot `cond` value-sorted, slot `body` `Stmt`-sorted). The loop rule needs an invariant. It is *not* re-specified here; it plugs into the existing `LoopInvariantMemento`. The op carries a third logical slot `inv`, supplied not by a sub-term but by a `LoopInvariantMemento` whose `loopCid` matches the loop's content CID (LoopInvariantMemento §0, §5). Until that memento is present in the pool, the function contract carries the `opaque_loop` effect and `wp` of any term containing the loop is **not computable** — the contract refuses downstream composition, by design. When the memento is present, the partial-correctness rule is:

```
wp_rule(Q)  ≡  inv
             ∧  ∀state. ( (inv ∧ cond)  ⇒  wp_body(inv) )
             ∧  ∀state. ( (inv ∧ ¬cond)  ⇒  Q )
```

and, when the memento also carries a `decreasingFunction`, the standard well-foundedness conjunct for total correctness. So `concept:while` is the canonical example of an op whose `wp_rule` is *conditionally* present: schema-level it is always the rule above, but the rule references `inv`, which is bound by a memento, not a sub-term, and the absence of that memento is the principled refusal point. Same shape applies to `unresolved_call` / `opaque_call`: an op standing for a call whose callee contract has not landed carries that effect and its `wp` is not computable until the callee contract is in the pool, at which point `wp(call f(args), Q) = f.pre[formals := args] ∧ (f.post[formals := args] ⇒ Q)` is the standard rule.

**A function contract's `wp`** is the `wp` of its body term, computed by walking the body with the per-op rules. It is *derived, not authored*. The contract memento stores `pre` and `post`; the `wp` is `wp(body_term, post)` evaluated by §2's algorithm. (The repo already has `menagerie/c11-language-signature/example/foo.expected-wp-contract.json`, the computed-wp form of `foo.contract.json` — that artifact stops being a hand-written expectation file and becomes the canonical output of the evaluator run on `foo.term.json`.)

### §1.4 CDDL

Following the LoopInvariantMemento style. `wp_rule` is an `ir-formula` over the op's formals plus the reserved meta-variables `Q` and `wp_<slot>`:

```cddl
; Imports:
;   ir-formula   ; from 2026-04-30-ir-formal-grammar.md, extended per §2.3

operation-contract = {
  kind:        "operation-contract",
  operator:    tstr,
  arity:       [* tstr],
  result:      tstr,
  arity_shape: arity-shape,
  ? pre:       ir-formula,            ; unchanged
  ? post:      ir-formula,            ; unchanged
  ? wp_rule:   ir-formula,            ; NEW; REQUIRED for Stmt-sorted ops, OPTIONAL (synthesizable) for value-ops
  ? wp_note:   tstr                   ; RENAMED from `wp`; human-readable only; non-load-bearing; MUST be omitted (not null) when absent
}
```

The `wp_rule` participates in the operation contract's JCS-canonical bytes and therefore in its CID. `wp_note` does too (it is a string in the canonical object), which means renaming `wp` to `wp_note` is a CID-changing edit and is a successor mint with `refines = <old CID>` per LSP §4.4, the same as the desugaring-equation tagging in the Desugaring spec §2.

## §2: The `wp`-evaluator

### §2.1 The algorithm

`wp` is compositional: `wp(t, Q)` is computed by structural recursion over the term `t`, looking up each op's `wp_rule` (authored, or synthesized for value-ops per §1.2) and instantiating it with `Q` and the recursively computed `wp`s of `t`'s sub-terms.

```
wp(t, Q):
  match t:
    var v          ->  Q[result_value := v]                 # leaf: substitute the value into Q
    const c        ->  Q[result_value := c]
    op(name, args) ->
        contract  := lookup_operation_contract(name)
        rule      := contract.wp_rule  or  synthesize_value_rule(contract)
        if rule references inv-from-a-loop-memento and no matching LoopInvariantMemento in pool:
            return Refusal::OpaqueLoop(loop_cid)
        if name is an unresolved call and no callee FunctionContractMemento in pool:
            return Refusal::OpaqueCall(callee)
        for each Stmt-sorted slot s of the op, with sub-term args[s]:
            wp_s := (Q' -> wp(args[s], Q'))                  # the slot transformer, a function on formulas
        for each value-sorted slot s of the op, with sub-term args[s]:
            v_s  := value_expr_of(args[s])                   # the value the slot evaluates to
        return rule [ Q := Q,  wp_<s> := wp_s for each Stmt slot,  formal_s := v_s for each value slot ]
```

The result is a formula in the same IR-formula language. The recursion bottoms out at `var`/`const` leaves and at ops with no `Stmt`-sorted slots (value-ops), so it terminates on every finite term — and every term is finite (paper 13 §7: a program of any interest is a bounded tree over a finite alphabet). The one place finiteness of the *term* does not give termination of `wp` is a loop: a loop op's body slot is a sub-term, but the loop rule does not recurse "into the loop forever" — it consults the `LoopInvariantMemento` and produces the three-conjunct formula above in one step, or refuses. So the evaluator is total: for every term it either returns a formula or returns a `Refusal` naming the missing memento. (The refusal is not a failure of the evaluator; it is the evaluator correctly reporting that the contract is not yet load-bearing, the same posture as PTP §3.)

### §2.2 Where it lives

A new `libsugar::wp` module. It is the *consumer* of `wp_rule` data; it is the outer recursion above plus the rule-instantiation. `walk`'s existing `wp.rs` — the body-level Dijkstra propagator that `compose.rs` already duplicates from — is refactored to call `libsugar::wp` rather than carry its own hardcoded per-op match-arms. The duplication that `compose.rs` documents ("duplicated from walk's wp.rs and canonical.rs so this module is...") collapses: `compose`, `walk`, the transport discharge, and the desugaring-equation check all consume the one `libsugar::wp`. This is the same consolidation move CCP made for contract composition: one primitive, every consumer.

### §2.3 The two grammar additions

The evaluator needs the IR formula grammar to express two things it cannot today:

1. **`substitute`** — an explicit, capture-avoiding, single-variable substitution on a formula: `{ "kind": "substitute", "target": <ir-formula>, "var": <name>, "term": <ir-term> }`, meaning "`target` with `var` replaced by `term`." This is what `Q[result_value := value_expr]` is. It can always be eliminated by performing the substitution when both `target` and `term` are ground; it is needed as a *node* because in a `wp_rule` schema the `target` is the meta-variable `Q`, not yet known.
2. **`apply`** — application of a slot transformer meta-variable to one formula argument: `{ "kind": "apply", "fn": "wp_<slot>", "args": [ <ir-formula> ] }`. When the evaluator instantiates `wp_<slot>` with the actual slot transformer, `apply(wp_<slot>, X)` reduces to `wp(args[slot], X)`, which is a concrete formula.

Both are conservative grammar extensions (an existing IR formula is still an IR formula), both reduce to the existing grammar when fully instantiated, both follow the locked-key-order discipline of the grammar spec. They are the smallest addition that lets a `wp_rule` be written in the language the rest of the substrate already speaks.

### §2.4 Determinism

The computed `wp` is canonicalized (`sugar-canonicalizer`, JCS) and content-addressed, so the evaluator MUST be deterministic: same term, same pool, same `wp`-formula bytes. It is — the recursion order is fixed by the term's slot order (arity shape), substitution is deterministic, rule lookup is by content CID. No solver call happens during evaluation; the evaluator produces a *formula*, and solver calls happen later, at the discharge step (§3), exactly as `wp` propagation in walk produces obligations now and defers implication-checking to the portfolio.

## §3: What the discharge check becomes

### §3.1 The current state

`mint_language_morphisms.py` discharges a morphism `<lang>:op → concept:op` by, in order:

1. `canonicalizer-alpha-equivalence-plus-representation-map` — apply the morphism's `operator_map`, `renaming_map`, `representation_map`, `literal_map` to the whole `<lang>:op` contract and check the canonical CID lands exactly on `concept:op`'s CID; or, failing that,
2. structural `⊑` relaxations: (a) `wp-text abstraction` — "specs differ only in `post.wp`, discharge anyway, `wp` carries no semantic weight"; (b) `pre-weakening` — "`lang_pre = true`, specs differ only in `{pre, post.wp}`, discharge anyway."

(b) is sound but a workaround; (a) is sound *only because `wp` is prose* and prose is being ignored on purpose. Once `wp` is a real rule, ignoring it is no longer sound, and "specs differ only in `wp`" is no longer a non-event.

### §3.2 The replacement: a real refinement check

A morphism `φ : <lang>:op → concept:op` discharges iff `φ` maps `<lang>:op`'s `wp_rule` to a *refinement* of `concept:op`'s `wp_rule`. Concretely, for every postcondition `Q`:

```
wp(concept:op, Q)   ⇒   φ( wp(<lang>:op, Q) )
```

read: the concept op's `wp` is at least as strong as the (φ-translated) lang op's `wp`, i.e. the lang op works in at least as many contexts as the concept op — wherever the concept op is usable, the lang op is too, which is exactly what "the lang op refines the concept op" means for a substitutability claim (paper 17 §3: substitutability is a discharged path; the path is this implication). This is checked by Z3 — the *same* Z3 the existing `compose` uses for pre/post implications, with `Q` universally quantified (the rule is a transformer, so the obligation is `∀Q. ...`; in practice, since both sides are `pre ∧ Q[...]`-shaped or boolean combinations of slot transformers, the `∀Q` discharges by structural matching on `Q` plus a residual implication on the `pre`/value parts, the same way `walk` discharges `wp ⇒ pre` obligations now).

### §3.3 It subsumes the relaxations

- **`canonicalizer-alpha-equivalence`** becomes the trivial case: if `φ(<lang>:op)`'s CID equals `concept:op`'s CID, the two `wp_rule`s are byte-identical, hence α-equivalent, hence `wp(concept) ⇔ wp(lang)`, hence the implication holds reflexively. No special-casing; it falls out.
- **`wp-text abstraction`** dissolves entirely: there is no prose `wp` to mismatch. There is `wp_note`, which is non-load-bearing and not in the obligation; and there is `wp_rule`, which *is* in the obligation, and you do not "abstract it away," you prove one refines the other. The relaxation row disappears because the thing it relaxed no longer exists.
- **`pre-weakening`** becomes a *derived corollary*, not a primitive. When `<lang>:op` has `pre = true`, its synthesized `wp_rule` is `true ∧ Q[result := v] = Q[result := v]`. The concept op's `wp_rule` is `concept_pre ∧ Q[result := v]`. The obligation `concept_pre ∧ Q[result := v] ⇒ Q[result := v]` is a one-line tautology that Z3 discharges in microseconds. So the special-case widening becomes a theorem the solver proves, like any other.

The whole relaxation table collapses into rows of "Z3 discharged `∀Q. wp(concept:op,Q) ⇒ φ(wp(lang:op,Q))`." And — the point — the prose-`wp` transport gaps *dissolve*: two ops that "differ only in `wp` prose" are now two ops with `wp_rule`s, and either one refines the other (discharge, no gap) or it does not (a *real* gap, with a solver-checked reason, not a sentence-comparison artifact). The estimate is that the prose-`wp` rows in `transport-gaps.md` — roughly half of the open rows — close on the first run of the reworked discharge.

### §3.4 Parametric rules: refining a schema, not a ground formula

For a control-flow op the `wp_rule` is parametric over the slot transformers `wp_<slot>`. The refinement check is then on the *rule schema*, not on a ground formula. Two equivalent handlings, pick one:

- **Universally quantify over the slot transformers.** Treat each `wp_<slot>` as an uninterpreted function symbol, assert `∀Q ∀wp_then ∀wp_else. ( wp_then, wp_else, Q version of concept rule )  ⇒  φ( same of lang rule )`, hand it to Z3 with the function symbols uninterpreted. For the Dijkstra `if` rule this is `∀Q,f,g. ((c⇒f(Q))∧(¬c⇒g(Q))) ⇒ φ((c'⇒f(Q))∧(¬c'⇒g(Q)))`, which reduces to the conditions' relationship under `φ` — exactly "the lang `if` and the concept `if` agree on which branch fires."
- **Check the rule structurally.** The two rules are terms with holes; if `φ` maps the lang rule's tree onto the concept rule's tree node-for-node with the slot meta-variables mapped consistently, they are the same transformer and the morphism discharges with no solver call at all (this is the common case for the primitive control-flow ops `if`, `seq`, `return`, `skip`, `eq` — the ones already in `PRIMITIVE_STEMS` in `mint_language_morphisms.py`).

The recommendation: try the structural check first (fast, no solver), fall back to the universally-quantified Z3 check, and only then record a gap. That mirrors the current "canonicalizer-equality first, then relaxations" order, but every step is now sound for a real `wp`.

## §4: The migration path

The 10 source lifters (c11, csharp, go, python, typescript, zig, ruby, php, java, rust) and the `concept:*` hub move from prose `wp` to `wp_rule`. It is a transition, not a flag-day: `wp_note` (the renamed prose) coexists with `wp_rule` for the whole window; the canonicalizer accepts `wp_note`-only contracts during it; the discharge rework is the *last* step, so transport keeps working on the old basis until the new basis is fully in place.

Order:

1. **Schema + grammar + evaluator first.** Add `wp_rule` and `wp_note` to the operation-contract schema in AMP/LSP and the CDDL; add the `substitute` and `apply` nodes to the IR formal grammar (§2.3); land `libsugar::wp` (the evaluator, §2) with tests, including the `foo.term.json` → computed-wp test that replaces the hand-written `foo.expected-wp-contract.json`; refactor `walk`'s `wp.rs` and `compose.rs` to consume `libsugar::wp`.
2. **Migrate the `concept:*` hub ops.** Author `wp_rule` for the ~10-20 control-flow concept ops (`conditional`/`if`, `seq`, `while`, `return`, `break`, `continue`, `try`/`catch`/`finally`, `throw`, `call`, `assign`); confirm value-op `wp_rule`s synthesize correctly from existing `pre`/`post`; rename `wp` → `wp_note` on the hub ops. This is post-#612 and post the hub-cleanup that #612 set up.
3. **Migrate the lifters, one at a time.** For each language, author `wp_rule` on its control-flow ops, rename `wp` → `wp_note` on all its ops, confirm value-ops synthesize. c11 and rust first (they have the most primitive-op coverage and the closest match to the hub, so they shake the evaluator out hardest), then the rest. Each lifter is independently mergeable; the hub already works against the old discharge, so a half-migrated set is not broken, only half-strict.
4. **Flip the discharge.** Replace `mint_language_morphisms.py`'s relaxation strategies with the §3 refinement check (structural-first, then Z3-quantified, then gap). Re-run; regenerate `transport-gaps.md`; publish the closure report (the count of prose-`wp` rows that closed).
5. **Move the desugaring-equation obligation onto the same machinery.** Desugaring spec §1.2 / §2.1 says a desugaring equation must satisfy `wp(lhs, Q) ≡ wp(rhs, Q) ∀Q`. Today that obligation is discharged by the equation portfolio treating it as "the same machinery as any `EquationMemento`," which in practice still leans on prose `wp` agreement. Once `wp_rule` is real, the obligation becomes literally `wp(lhs_term, Q) ⇔ wp(rhs_term, Q)` evaluated by `libsugar::wp` on both sides and handed to the portfolio as a bi-implication — machine-discharged, not asserted. A desugaring is then *demonstrably* a homomorphism of the term algebra, not asserted to be one.
6. **Close the migration window.** Once all 10 lifters and the hub carry `wp_rule`, the canonicalizer stops accepting `wp_note`-only operation contracts; past this point an op with no `wp_rule` (and not synthesizable as a value-op) is a non-load-bearing contract that refuses composition. `wp_note` stays as the optional human annotation forever.

Estimated PR count: **8 to 10**. (1) schema + CDDL + grammar nodes + canonicalizer key-order; (2) `libsugar::wp` evaluator + tests + `walk`/`compose` refactor; (3) `concept:*` hub op `wp_rule` mint + `wp` → `wp_note` rename; (4) lifter sweep — either one PR per lifter (10 small PRs) or one sweep PR with all ten plus a per-lifter conformance check (call it 1-2 here, so the band stays 8-10 at the low end and ~17 if every lifter is its own PR); (5) `mint_language_morphisms.py` discharge rework + `transport-gaps.md` regen + closure report; (6) desugaring-equation obligation moved to the `wp_rule` bi-implication; (7) migration-window close (canonicalizer rejects `wp_note`-only). Realistically Tsavo scopes this as 8-10 if the lifter sweep is one or two PRs, ~17 if each lifter is its own.

## §5: What it costs and what it's worth

**The work.** The schema field and the two grammar nodes (small). The evaluator (§2) — a structural recursion plus rule instantiation, roughly the size of `walk`'s existing `wp.rs`, minus the per-language match-arms it deletes. The `concept:*` hub control-flow `wp_rule`s (~10-20 hand-authored rules, one-time). Ten lifter migrations (mostly renames plus ~10-20 control-flow rules each, much of it copy-from-hub). The discharge rework in `mint_language_morphisms.py` (replace two relaxation strategies with one refinement check). The desugaring-obligation move (point the existing obligation at the evaluator). Call it 8-10 PRs, weeks not months, no architecture invented — the IR formula language, the canonicalizer, the Z3 wiring, the `walk` recursion, the `LoopInvariantMemento`, the `opaque_loop` effect all already exist; this connects them.

**The payoff.** A real `⊑` everywhere a contract meets a contract: composition (CCP) stops byte-comparing and starts proving; transport (PTP) stops relaxing and starts refining; the prose-`wp` transport gaps — about half the open rows — close on the first reworked run. Contract composition becomes provably correct rather than canonicalizer-equal-or-relaxed. The desugaring layer's `wp`-preservation obligations become machine-discharged: a desugaring is *shown* to be a homomorphism, not declared one. And it is the realization of paper 17's "the `wp` rule is the proof" — the operation's contract theory now *has* a proof object, addressed by its CID, the same as every other proof in the substrate, instead of a sentence in English standing in for one.

**The hard parts, named honestly.**

- *Loop invariants.* A loop op gets a real `wp` exactly when a `LoopInvariantMemento` matching its loop CID is in the pool; until then it stays `opaque_loop` and the contract refuses composition. That is not a gap in this proposal — it is the proposal correctly inheriting the loop-invariant story that already exists. But it does mean "this function's `wp`" is partial in practice: programs with un-annotated loops have un-computable `wp` and that is by design. The honest statement is: this makes the *refusal* precise (it names the loop CID whose memento is missing), it does not make the loop go away.
- *Function pointers / unresolved calls.* `wp` of a call whose callee contract has not landed is not computable; the op carries `opaque_call` and refuses composition until the callee `FunctionContractMemento` is in the pool. For a fully unresolved indirect call (no candidate set at all) `wp` stays opaque indefinitely — that is a real limit, and the proposal does not pretend otherwise; it makes the limit explicit at the contract level instead of letting a prose `wp` paper over it.
- *Polymorphic ops.* `python:+`'s `wp` depends on the operand sorts (int add vs. string concat vs. list extend). The recommendation: sort-resolution happens *before* `wp` evaluation — the lifter emits a sort-resolved op (`python:+@(Int,Int)`, `python:+@(Str,Str)`, ...) and each resolved op has its own `wp_rule`. The alternative — one `python:+` op with a sort-guarded multi-rule `wp_rule` (a `match`-on-operand-sorts node in the rule) — is more compact but pushes a case split into the formula language and the solver; sort-resolution-first keeps the rule per op simple and matches how the lifters already disambiguate. Either is sound; this proposal recommends sort-resolution-first and flags the choice for review.

## §6: The relationship to the rest

**Paper 07 (bug classes as missing edges).** `wp` *is* the structural eliminator: paper 07's "weakest-precondition propagation is the algorithm," Dijkstra 1975 unchanged, the deterministic finite walk rooted at allocations with implication-checking deferred to solvers. This spec is what makes that algorithm consume *data* (per-op rules carried in contracts) instead of *code* (per-language match-arms), so the same propagation runs over any language whose ops carry `wp_rule`s — which is the whole point of "bug classes are missing *edges*": an edge is a discharged implication produced by `wp` propagation, and you cannot have the edge without a real `wp`.

**Paper 13 (programming languages as content-addressed algebras).** Paper 13's homomorphism obligation — a language morphism preserves the operation contracts — gets its precise statement: a morphism preserves `wp` *is* "preserves the contract." Paper 13 §4.7's lineage (MLIR's dialects + CompCert's correctness square + Isabelle-`transfer` + HoTT's orbit-as-name + content-addressing) gets its missing piece: the correctness square is discharged by the §3 refinement check on `wp_rule`s, not by byte-comparison with relaxations. The "two `if`s are the same construct" CID-addressable fact becomes "the morphism between them discharges `∀Q. wp(if,Q) ⇒ φ(wp(if',Q))`."

**The desugaring spec (#601, 2026-05-11).** Its §1.2 soundness condition — a desugaring rewrite preserves `wp` — becomes machine-discharged: `wp(lhs_term, Q) ⇔ wp(rhs_term, Q)` evaluated by `libsugar::wp` on both sides, handed to the portfolio as a bi-implication, receipted like any equation. *Supra omnia, rectum*: the substrate stops *trusting* that a rewrite labeled "desugaring" preserves `wp` and starts *checking* it; a `wp`-changing rewrite cannot wear the label.

**Cross-language transport (#609/#612, PTP).** PTP §2.3/§2.4's morphism discharge becomes a real `⊑`: the `canonicalizer-alpha-equivalence-plus-representation-map` strategy stays as the trivial reflexive case; the two structural relaxations are replaced by the §3 refinement check; the reverse use in §2.4 (target spoke) discharges when the refinement holds both directions (`wp` equivalence, not just one-way). The transport-gaps shrink to the *real* mismatches.

**The `opaque_loop` / `unresolved_call` effects.** These are exactly the places where `wp` is genuinely not-yet-computable, and a contract carrying one refuses downstream composition until a memento lands (`LoopInvariantMemento` for the loop, `FunctionContractMemento` for the callee). This spec does not change that; it makes the evaluator (§2.1) return a `Refusal` naming the missing memento instead of producing a wrong `wp` — the opacity effects become the evaluator's explicit "I cannot compute this yet, here is what I need" rather than a silent prose stand-in.

**Lineage.** Hoare's axiomatic semantics is 1969; Dijkstra published `wp` in 1975 (*Guarded Commands, Nondeterminacy and Formal Derivation of Programs*; *A Discipline of Programming*, 1976); predicate-transformer semantics — `wp` as a function from postconditions to preconditions, composed by structural recursion over the program — is theirs, and unchanged here. What this spec adds is not a new logic. It is: the predicate transformer carried as content-addressed data per operation rather than as code per language; the homomorphism obligation (a morphism preserves `wp`) discharged by a solver rather than asserted; and the whole thing federated, so two parties who never coordinated compute the same `wp_rule` CID and the same refinement obligation. Cousot 1977 is the math root (this is abstract interpretation's predicate-transformer instance, lifted to federation), paper 07 said it first; the algorithm is fifty years old; the federation is the new bit.

## §7: Why this matters (the closing principle)

A contract that says `"wp": "mathematical integer addition when no overflow holds"` is a contract that has *described* its proof and not *carried* it. The substrate's whole posture — *Supra omnia, rectum*, paper 11's proof-as-primary-artifact, paper 17's `compose = wp, federated` — is that the proof is the thing you ship, content-addressed, re-verifiable by a stranger, not a sentence you trust. Making `wp` a rule in the formula language the substrate already speaks is the move that lets the operation contract *be* the proof of its own theory instead of an English gloss of one. Once it lands, `⊑` is real wherever two contracts meet, the transport gaps that are prose artifacts dissolve, composition stops being a byte-comparison, and desugaring stops being a label you trust. It is the single change that makes the contract stratum mean what the rest of the substrate already assumes it means.

— T Savo
