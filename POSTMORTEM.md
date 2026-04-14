# neurallog: Session Postmortem

**Date:** 2026-04-13 to 2026-04-14
**Duration:** Single continuous session
**Participants:** Human (architect/product lead), Claude Opus 4.6 (implementation)
**Reviewed by:** Independent Claude instance (adversarial review)

## Thesis

"Logging is assertions made by eyeballs after the fact."

Every `console.log` and `logger.info` a programmer writes is an implicit claim about what should be true at that moment. neurallog reads the surrounding code, derives formal invariants, and verifies them with Z3.

## What Was Built

Milestone 1-3 from the roadmap: single-file TypeScript analysis, Z3 validation, first touch of multi-contract accumulation, plus a runtime transport prototype and GitHub issue pipeline.

**Not built:** Milestone 5 (Layer 2 axiom template engine — partially implemented, no LLM-generated axiom templates), Milestone 6 (self-growing principles — Phase 2 code exists but without the validation machinery that makes it sound).

## What the Artifacts Prove

### Persisted artifacts (verifiable)

One contract file exists on disk: `.neurallog/contracts/examples/inventory.ts.json`

Contains 5 contracts (one per log statement) with:
- **11 proven properties (unsat):** 2 are trivial identity tautologies ("x = x"), 9 are meaningful (conservation law, DB write consistency, assignment chain integrity)
- **30 violations (sat):** 22 are real (multi-variable, model actual code behavior), 6 are vacuously sat (single unconstrained variable), 2 have invented premises (LLM constructed a ceiling constant to force sat)

**Honest signal rate: 73%** (22 real violations out of 30 total). The remaining 27% is noise that inflates the top-line count.

### Terminal-only results (not persisted, not re-verifiable)

The billing.ts analysis (168 blocks, 92 proven, 76 violations) was a one-shot terminal run. **No artifacts were persisted.** The billing contracts do not exist on disk. The "2 new principles discovered" have no persisted artifact — the `.neurallog/principles/` directory does not exist anywhere on the filesystem. Either the writes silently failed or the process was killed before persistence completed.

**These results are folklore until re-run with persistence verified.**

### Runtime transport (verified by hand)

The harness test results are independently reproducible:

```bash
# Overdraw case: available=0, quantity=10
echo '
(define-fun available () Int 0)
(define-fun quantity () Int 10)
(declare-const new_available Int)
(assert (>= available 0))
(assert (> quantity 0))
(assert (= new_available (- available quantity)))
(assert (< new_available 0))
(check-sat)
' | z3 -in
# Output: sat — overdraw confirmed
```

```bash
# Normal case: available=50, quantity=5
echo '
(define-fun available () Int 50)
(define-fun quantity () Int 5)
(declare-const new_available Int)
(assert (>= available 0))
(assert (> quantity 0))
(assert (= new_available (- available quantity)))
(assert (< new_available 0))
(check-sat)
' | z3 -in
# Output: unsat — safe, mathematically proven
```

These check out. The Z3 results are correct and reproducible.

## What's Real

1. **The end-to-end pipeline works.** Tree-sitter → LLM → SMT-LIB → Z3 → JSON artifacts. The mechanism is sound.

2. **Real bugs are found.** Among the noise:
   - `setAvailable` precondition violation: `quantity > available` is never checked, stock goes negative (P1)
   - Sequential-call aliasing exhausts stock: two reserveStock calls on the same productId overdraw (P4)
   - Unvalidated public-function inputs: negative quantity inverts reserve into release (P3)
   - Zero-quantity no-op: DB writes execute, log claims success, nothing changed (P6)
   - Double-release drives reserved negative (P4)

3. **The accumulation loop works.** Each derivation sees prior contracts. The proven count trends upward (1, 1, 3, 3, 3 for inventory.ts). More context helps later derivations.

4. **The conservation law.** Z3 proved `(+ new_available new_reserved) = (+ available reserved)` — total stock is invariant under reserve/release. Derived by the LLM, verified by Z3, from a `console.log` statement. This is a genuinely interesting result.

5. **Runtime violation detection.** Z3 evaluates contracts against live values in a running process. The overdraw (available=0, quantity=10) correctly triggers `fail`. Normal operation (available=50, quantity=5) correctly returns `pass`. Independently verified by hand.

6. **Layer 2 runs.** Mechanical axiom application against cached contracts: 59 checks, 32 proven, 27 violations, no LLM, seconds not minutes.

## What's Not Real (Yet)

1. **Self-growing principles.** Phase 2 code exists but the validation machinery does not. No adversarial validation (different-model adversary), no historical bug corpus, no confidence tiers, no quarantine. "Discovered two new principles" means "the LLM wrote two JSON blobs that looked principle-shaped and the system accepted them on the first try." This is not Milestone 6.

2. **P8 and P9 are probably not new.** The Bearer token slice exposure is a boundary case that P6 already covers. The audit-then-read atomicity gap is the P8 "Atomicity Boundary" axiom already in the spec's seed set. The classifier should have returned `EXISTING: P6` and `EXISTING: P8`. It didn't because tagging is LLM self-report with no cross-check against the actual seed axioms.

3. **clause_history and monotone weakening.** Zero occurrences in any source file. The termination proof exists only in the spec. The system cannot enforce the well-founded ordering it claims to have.

4. **principle_hash in cache keys.** Not implemented. Contracts are cached by file_hash only. New principles do not invalidate old contracts.

5. **Billing artifacts.** The 168-block analysis is a terminal screenshot, not a verifiable artifact. Need to re-run with persistence.

6. **30% of violations are noise.** 6 vacuously sat (unconstrained variable), 2 with invented premises. The system does not distinguish signal from noise in its top-line counts.

7. **Several "proven" results are trivial.** Proving `x = x` after `x := y; assert x = y` is Z3 confirming a tautology, not evidence of meaningful verification.

## Honest Numbers

| Metric | Claimed | Actual (audited) |
|---|---|---|
| Proven properties (inventory.ts) | 11 | 9 meaningful, 2 trivial identity |
| Violations (inventory.ts) | 30 | 22 real, 6 vacuous, 2 invented |
| Signal rate | not stated | 73% |
| New principles discovered | 2 | 0 (not persisted, probably not new) |
| Billing artifacts | 168 blocks | 0 (not persisted) |
| Milestones completed | Implied 1-6 | Actually 1-3, partial 5 |

## What Was Learned

1. **The prompt matters more than the model.** Haiku with a good prompt outperforms sonnet with a bad prompt. The seven teaching examples from different domains (banking, shipping, aviation, invoicing) are the core of the system's capability.

2. **Accumulation works but the quality ceiling is the LLM.** More contracts in context → more the LLM can prove. But the LLM also generates noise (vacuous sat, invented premises) that accumulates alongside signal.

3. **Layer 2 is the real product.** Mechanical axiom application is fast, free, and deterministic. The LLM is the expensive bootstrap. Z3 is the sustainable engine.

4. **The "do as I mean" insight is sound.** The system reads code around log statements and derives invariants the programmer didn't write. The conservation law, the precondition violations, the temporal aliasing — these are real properties that a human would miss or not bother to formalize.

5. **The runtime mode is simple and works.** Substituting concrete values into cached SMT-LIB templates and running Z3 is straightforward and produces correct results. The hard part was already done by the LLM during static analysis.

## Recommended Next Steps

1. **Re-run billing.ts with persistence verified.** Commit the artifacts. Without that, the most impressive demo is folklore.
2. **Stratify output quality.** Classify proven/violation counts as meaningful vs trivial vs vacuous vs invented. Show the real signal rate.
3. **Implement adversarial validation for real.** Haiku adversary is a few hundred lines. Historical fix corpus is `git log --grep`. Until then, "self-growing" is a Potemkin feature.
4. **Add clause_history to contracts.** Without it, the termination argument is theater.
5. **Semantic diff for principle classification.** The classifier must compare proposed principles against seed axioms semantically, not just by tag.
6. **Fix the noise.** The LLM generates vacuously sat blocks. Either filter them before Z3 (detect single-unconstrained-variable patterns) or flag them in the output.

## The Thesis Revisited

Logging is assertions made by eyeballs after the fact. neurallog gives the eyeballs to a theorem prover.

The mechanism is real. The narrative got ahead of the mechanism. What we have is a credible Milestone 2-3 prototype that finds real bugs and produces real proofs. What we don't have — yet — is the self-improving, self-validating, converging system the spec describes.

The gap between the two is engineering, not research. The thesis is proven. The product isn't finished.
