# IR -> Lean 4 translator

Parallel to `src/ir/smt/`. Z3 and CVC5 are SMT-shaped verdict sources;
Lean 4 is the proof-assistant leg of the cross-paradigm composition test
(see `docs/specs/2026-04-29-the-semantic-envelope.md`). All three attach
as leaves to the same IR CID.

## Public surface

- `emitLean(formula)` — render a single `IrFormula` as a Lean 4 expression
  (no preamble, no theorem wrapper).
- `emitLeanTheorem({ axioms, assertion, name? })` — render full Lean
  source: `axiom` declarations for user sorts / ctors / uninterpreted
  predicates, then `theorem <name> : <prop> := by sorry`. The proof body
  is `sorry` so the file parses; `provideLeanProof` splices the user's
  proof in for `sorry`.

## What the FOL fragment covers

| IR feature | Lean output |
|------------|-------------|
| `forall` | `∀ (x : T), body` |
| `exists` | `∃ (x : T), body` |
| `and` | `(a ∧ b)` |
| `or` | `(a ∨ b)` |
| `not` | `(¬ body)` |
| `implies` | `(a → b)` |
| `iff` (library-desugared) | `((a → b) ∧ (b → a))` |
| `=` | `(a = b)` |
| `≠` | `(a ≠ b)` |
| `<` `≤` `>` `≥` | `(a < b)` etc. |
| `true` / `false` | `True` / `False` |
| Uninterpreted predicate | `(predName arg1 ...)` + `axiom predName : ... -> Prop` |
| Ctor term | `(ctorName arg1 ...)` + `axiom ctorName : ... -> Range` |
| User sort | bare identifier + `axiom Name : Type` |
| `Bool` / `Int` / `String` | Lean built-ins |
| Const literals | Lean numeric / string / boolean literals (negative ints annotated `(n : Int)`) |

## What the translator throws on

The discipline is "throw a structured error rather than silently
mistranslate." `LeanUnsupportedError` is raised on:

- `Real` sort — requires Mathlib (`Real` is not in plain Lean 4).
- `Set` sort — requires Mathlib (`Set` is not in plain Lean 4).
- `tuple` sort — out of FOL scope.
- `function` sort — out of FOL scope.
- `null`/`undefined` const — kits must model nullability as an explicit
  ctor, not a null literal.

Kits that depend on Mathlib can compose their own sort emitter to lift
these restrictions.

## Lean-specific quirks (vs SMT-LIB)

- **Uninterpreted relations.** SMT-LIB's `member` / `subset` map to
  uninterpreted predicate symbols. Lean handles them the same way: an
  `axiom predName : Arg1 -> Arg2 -> Prop` declaration plus prefix
  application. Kits must supply axioms for content; the translator
  declares only signatures.
- **Identifier characters.** SMT-LIB allows `$` in identifiers (we use
  `$<depth>` to disambiguate clashing binders); Lean does not. The Lean
  binder uniquification suffix is `__d<depth>` instead.
- **Negative integer literals.** Plain `-3` parses ambiguously in
  argument position; we emit `(-3 : Int)` so Lean knows it's an Int.
- **Theorem naming.** A theorem needs a name. The translator derives one
  deterministically as `prop_<sha256-prefix-of-rendered-prop>` so
  identical formulas yield identical names. Callers may pass their own
  name explicitly.
- **Axioms folded into the statement.** The kit's axioms are folded into
  the theorem proposition as `(ax1) → (ax2) → ... → assertion`, so the
  proof receives them as hypotheses. We do NOT emit the axioms as Lean
  `axiom` declarations of asserted propositions, because that would let
  the proof rely on potentially-inconsistent axiomatic content the kit
  hasn't justified.

## Out of scope (for v1)

- Mathlib-dependent sorts (`Real`, `Set`).
- Refutation via Lean (proving `¬ P`). The current Action only checks
  whether a supplied proof of `P` goes through; failure to check is
  mapped to `undecidable`, not `violated`. Z3 sat-with-counterexample is
  the only verdict source that yields `violated` today.
- Full Mathlib lemma name compatibility. SMT-LIB and Mathlib spell the
  same operator differently (e.g., SMT `set.member` vs Mathlib `Set.mem`).
  Kits that want Mathlib-style lemma names should compose their own
  predicate-name mapping on top of `emitLeanTheorem`.
