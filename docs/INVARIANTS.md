# ProvekIt Invariants

The load-bearing laws of the substrate. Violating one is the bug, not a judgment call.
These were re-derived many times before being written down; check work against them.

## I. What correctness is

1. **Correctness is four parts, relative to asserted claims.** A spec exists; the spec is
   coherent; a program satisfies it; a witness demonstrates it. Not "compiles," not "green
   tests," not coverage — those are everyone else's gates, assumed here as axioms.

2. **We see only claims.** A program enters our universe solely through its assertions. A
   claim is an operator over operands — a first-order-logic atom. A line that asserts nothing
   (a binding, setup, `return;`) is *invisible*: not a gap, not N/A, not in any denominator.
   No contract = nothing to prove.

## II. What we are not

3. **Not a compiler.** Compiler facts are axioms: if it compiled, types, borrows, drops,
   Send/Sync, lifetimes all hold — given, never re-checked. By construction we only ever lift
   from compiling code, so every contract is born type-valid; an ill-typed composition cannot
   arise upstream of us.

4. **Not a type system.** We never validate types — the compiler did. We carry canonical
   *identity* (to name subjects and keys), never type-validation. The moment a "fix" looks
   like type relationships or a type hierarchy, it is reinventing the compiler — stop.

5. **Not an effects theory.** We never model effects or assume determinism. Pure/concrete
   operands coalesce (same subject); impure/symbolic operands stay locus-distinct (their own
   subject). Both are sound; we simply decline to assume two impure calls are equal.

6. **Not a correctness checker.** The solver decides — z3, coq, vampire, maude. We lift to IR,
   form the obligation, and route it to a prover. We do not implement theories: floats are
   z3's FP theory, strings z3's string theory, quantifiers go to vampire/coq.

## III. The spec

7. **The vendor's tests are the spec.** The contract is lifted from the library's own tests.
   No declarations, no annotations (no JSR-380, no hand-added `#[requires]`), no authored
   contracts, zero changes to the code under proof. If the vendor never asserted it, we are
   silent on it — and that is correct, not a gap.

## IV. The layering

8. **Lifter -> ProofIR -> CLI -> solver, and language lives in exactly one place.** All
   language-specifics live in the per-language lifter; it emits language-agnostic ProofIR; the
   CLI / verifier / libsugar are language-blind (no rustiness, no per-language logic); the
   solver discharges. Test: the IR for a value is the canonical shape *any* lifter would emit,
   so it federates by CID.

## V. The logic

9. **ProofIR is first-order logic.** Atoms plus and/or/not/implies/forall/exists.
   Value-equality is just the simplest atom. The lift is a walk over the program structure
   (V, A, <=): bindings introduce operands (V, no claim), assertions are atoms (A, the claims),
   threaded in source order. Nested calls key recursively — the outermost call is the subject,
   inner calls are operands.

## VI. Identity and federation

10. **The CID is the identity. There are no hubs.** Identical canonical shape -> identical CID,
    automatically, across languages and across time. Federation and cross-language binding are
    byte-identical CIDs plus the bridge memento (post |= pre at the call edge) — never a
    concept layer, a naming registry, or an identity hub.

## VII. Soundness

11. **Sound by construction; conservative; one-directional.** We never falsePass. Identity is
    over-precise on purpose — carry the type argument, the concrete args, the locus — so
    distinct subjects never falsely coalesce. The only permitted failure mode is conservative
    over-refusal.

## VIII. The artifact

12. **The .proof is content-addressed and re-verifiable.** We do not ask for trust; we hand
    over a recomputable artifact. Everyone else asserts their code is correct; we `.proof` it.

## IX. How we work on it

13. **Lift changes are shared substrate: additive and backward-compatible.** Preserve existing
    `#euf#` keys; after any lift edit, confirm the sibling showcases still bind.

14. **Verify the mechanism, not the report.** Confirm the actual GitHub CI conclusion; read the
    runner for real `sugar` invocation; reject hardcoded verdicts and tautological self-checks.
    A local pass is not a CI pass.

15. **Never fabricate; report honest gaps.** A faked row, proof, or logo is worse than none.
    Honestly-scoped beats fake-full.

16. **Python is the reference — mirror, don't reinvent.** When a mirror is broken, diff it
    against the working reference; the bug is the diff. Do not generatively re-derive a
    mechanism that is already solved.
