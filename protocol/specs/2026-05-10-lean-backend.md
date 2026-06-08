# Lean 4 and mathlib Backend

**Status:** implementation note
**Date:** 2026-05-10
**Owner:** verifier crate
**Companion specs:** `2026-05-02-multi-solver-protocol-v2.md`, `2026-05-09-language-signature-protocol.md`
**Companion paper:** `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`

## 1. Scope

The Lean backend adds a Lean 4 plus mathlib seat to the prove portfolio. It is parallel to the existing Coq seat: the IR compiler lowers an `IrFormula` to a single Lean file, and the verifier adapter asks Lean's kernel to check that file through `lake env lean`.

The compiler imports `Mathlib`, states the obligation as `theorem sugar_obligation : <IR proposition>`, and supplies a tactic proof attempt using mathlib automation. The generated file also runs `#print axioms sugar_obligation` so the verifier can record the checked proof's trust base.

## 2. Coverage Declaration

The Lean compiler declares sound coverage for:

- `dependent_type` positions expressible as Lean dependent products.
- `categorical_structure` obligations whose Lean statement cites mathlib category theory, algebra, or free algebra facts.
- Higher order formulas expressible as Lean function types.
- General first order goals that Lean and mathlib automation can close.

Lean's kernel remains the authority. The compiler may emit a syntactically valid scaffold, but any proof that uses `sorry` is not a discharge.

## 3. Verdict Semantics

The Lean solver returns `Discharged` only when all of the following hold:

- `lake env lean <file.lean>` exits with status 0.
- The emitted file contains no `sorry`.
- The output from `#print axioms sugar_obligation` contains no `sorryAx`.
- Lean reports no errors.

Any failure, timeout, elaboration error, tactic failure, `sorry`, or `sorryAx` produces `Unknown` at the portfolio level, represented by `ObligationVerdict::Undecidable` in the Rust verifier.

Standard Lean and mathlib axioms are recorded, not rejected. They are part of the receipt's trust base.

## 4. Receipt Shape

Each Lean invocation records a receipt with:

- `leanVersion`: output from `lean --version`.
- `mathlibCommit`: the resolved mathlib revision from `lake-manifest.json`.
- `emittedFileCid`: `blake3-512:<hex>` over the emitted Lean file bytes, computed through `sugar-canonicalizer`.
- `axioms`: the axiom set reported by `#print axioms sugar_obligation`.

This matches the content-addressed receipt model: the proof text has a CID, the kernel version is explicit, the mathlib revision is explicit, and the axiom set is the proof's trust base.

## 5. Portfolio Composition

The Lean solver is registered with `ir_compiler = "lean"` and binary `lake`. In portfolio mode it runs beside z3, cvc5, Vampire, and Coq. In dispatch mode, dependent type and category theory positions may route directly to Lean through the `dependent-type` and `categorical-structure` dispatch keys.

Under the multi-solver protocol v2 model, each compiler remains the authority on what it translated soundly. Lean covers dependent and categorical positions that SMT compilers report as opaque. A `sorry` proof never covers a position.

## 6. LSP and Paper 13 Alignment

LSP section 2 defines the homomorphism obligation for language morphisms. LSP section 6 states that discharged morphisms compose when their homomorphism obligations compose.

Paper 13 supplies the categorical background:

- Lemma 2, morphism composition, aligns with LSP section 6.
- Lemma 3, initial algebra universality, aligns with mathlib's algebraic and categorical libraries.
- Lemma 6, effect signatures as embedded Lawvere theories, aligns with mathlib category theory and algebraic structure support.

The Lean backend is the portfolio seat that lets these obligations cite mathlib facts and have Lean's kernel check the resulting proof.
