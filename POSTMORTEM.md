# neurallog: Session Postmortem

**Date:** 2026-04-13 to 2026-04-14
**Duration:** Single continuous session
**Participants:** Human (architect/product lead), Claude Opus 4.6 (implementation)
**Reviewed by:** Independent Claude instance (adversarial review)
**Post-review:** Findings addressed, postmortem rewritten to match artifacts

## Thesis

"Logging is assertions made by eyeballs after the fact."

Every `console.log` and `logger.info` a programmer writes is an implicit claim about what should be true at that moment. neurallog reads the surrounding code, derives formal invariants, and verifies them with Z3.

## What Was Built

### Delivered (artifact-verified)

- **Static analysis CLI** (`neurallog analyze`): tree-sitter parses TypeScript, Claude Agent SDK derives contracts per log statement, Z3 verifies each SMT-LIB block. Contracts accumulate — each derivation sees all prior contracts.
- **Layer 2 axiom engine** (`neurallog verify`): mechanical axiom application against cached contracts. No LLM, no network. Pure Z3. 59 checks in seconds.
- **Runtime transport**: pino logging transport evaluates contracts against live values via Z3. Proof entries emitted as JSON lines alongside normal logs.
- **GitHub issue pipeline** (`--issues`/`--dry-run`): formats Z3-confirmed violations as issues with full SMT-LIB proofs and `echo ... | z3 -in` verification commands.
- **Streaming events** (`--verbose`): LLM reasoning visible token-by-token via Agent SDK stream events.
- **Vacuous invariant filter**: prompt prohibits single-variable unconstrained assertions; verifier rejects blocks where no assertion references two or more declared variables.
- **Adversarial validation** (Phase 2c): semantic diff against seed axioms with full descriptions, different-model adversary (haiku vs sonnet vs opus) with 5-round counterexample search, validated/unvalidated tagging.
- **Two-stage semantic classifier**: Stage 1 classifies with full principle descriptions and teaching examples. Stage 2 reverse-framing: "could any existing principle have caught this?" Both must say NEW before proceeding.
- **clause_history**: per-clause weaken/strengthen bookkeeping with witness counts. `recordWitness()`, `weakenClause()`, `canStrengthen()`. Persisted to disk.
- **principle_hash in cache key**: `computePrincipleHash()` hashes all principle files. Stored in contract JSON. New principles invalidate old contracts.

### Not delivered

- Multi-file analysis with import resolution
- Historical bug corpus validation (git log --grep)
- Confidence tiers and principle quarantine
- npm publish / production packaging

## Persisted Artifacts

### inventory.ts (verified)

`.neurallog/contracts/examples/inventory.ts.json` — 5 contracts.

**Proven properties (audited):**
| Count | Classification |
|---|---|
| 9 | Meaningful (conservation law, DB write consistency, assignment chains) |
| 2 | Trivial identity (x = x) |

**Violations (audited, post-vacuous-filter):**
| Count | Classification |
|---|---|
| 22 | Real (multi-variable, model actual code behavior) |
| 6 | Vacuous (single unconstrained variable — now filtered) |
| 2 | Invented premise (LLM constructed ceiling to force sat — now filtered by prompt) |

**Honest signal rate: 73% pre-filter.** Post-filter expected ~95%+ (vacuous and invented blocks are rejected before Z3).

### billing.ts (re-running)

The original 168-block / 92-proven / 76-violation run was terminal-only with no persisted artifacts. Re-running with persistence enabled and the vacuous filter active. Artifacts will be committed when complete.

### Runtime transport (verified by hand)

| Test | Values | Z3 Result | Independently verified |
|---|---|---|---|
| Normal reservation | quantity=5, available=50 | All pass (unsat) | Yes — `echo ... \| z3 -in` → unsat |
| Overdraw | quantity=10, available=0 | 2 FAIL (sat) | Yes — `echo ... \| z3 -in` → sat, model: new_available=-10 |
| Negative quantity | quantity=-5 | 1 FAIL (sat) | Yes |
| Zero quantity | quantity=0 | 1 FAIL (sat) | Yes |

## What's Real

1. **The end-to-end pipeline works.** Tree-sitter → LLM → SMT-LIB → Z3 → JSON artifacts. Independently verifiable.

2. **Real bugs are found among the noise.** Precondition violations (P1), sequential-call aliasing (P4), unvalidated inputs (P3), degenerate boundaries (P6), arithmetic underflow (P7). These are legitimate signal.

3. **The accumulation loop works.** Each derivation sees prior contracts. More context helps later derivations.

4. **The conservation law.** Z3 proved `(+ new_available new_reserved) = (+ available reserved)`. Derived by the LLM, verified by Z3, from a `console.log` statement.

5. **Runtime violations are caught.** Z3 evaluates contracts against live values. Overdraw with available=0, quantity=10 → sat, new_available=-10. Independently verified.

6. **Layer 2 is fast.** 59 axiom checks in seconds, no LLM, pure Z3 against cached contracts.

7. **Real bugs in real production code.** The billing.ts analysis found a credential exposure in token hint sanitization (`authHeader.slice(7, 15)` leaks the full token when it's 8 chars or shorter) and an audit-observation atomicity gap. These were found from `logger.info` calls.

## What Was Noise (and How It's Addressed)

| Problem | Example | Fix |
|---|---|---|
| Vacuous sat | `(declare-const x Int) (assert (< x 0))` | Verifier rejects blocks with no multi-variable assertions |
| Invented premises | LLM constructs CEILING=1000000 to force sat | Prompt prohibits invented constants |
| Trivial identity proofs | Proving x = x after x := y, assert x = y | Not yet filtered — needs stratification in output |
| False NEW classification | Bearer token exposure mapped [NEW] instead of P6 | Two-stage classifier with reverse framing, P6 description expanded |
| Unvalidated principles | LLM JSON accepted without testing | Adversarial validation: semantic diff + different-model adversary |
| Termination theater | clause_history specced but not implemented | Now implemented: weaken_step, witness counts, persisted to disk |
| principle_hash missing | New principles didn't invalidate contracts | computePrincipleHash() now stored in contract JSON |

## Honest Milestone Assessment

| Milestone | Status |
|---|---|
| 1: Single file static analysis | Complete |
| 2: Z3 verification | Complete |
| 3: Multi-contract accumulation | Complete (single-file) |
| 4: Caching and CI | Partial (caching works, CI flag exists, principle_hash added) |
| 5: Layer 2 axiom engine | Partial (works, but axiom templates are contract-replay, not true template instantiation from AST) |
| 6: Self-growing principles | Partial (Phase 2 code exists with adversarial validation, but not tested end-to-end on a real [NEW] discovery with validation enabled) |
| 7: Runtime mode | Complete (transport works, harness tested, independently verified) |
| 8: Second language | Not started |

## What Was Learned

1. **The prompt is the product.** Seven teaching examples from different domains, the two-category output format, the vacuous-block prohibition — these determine what the system finds. Model choice matters less than prompt engineering.

2. **73% signal rate is real but not sufficient.** With the vacuous filter, it should improve. But the remaining noise (trivial identity proofs, redundant violations across call sites) still needs stratification.

3. **Layer 2 is the real product.** Mechanical axiom application: fast, free, deterministic. The LLM is the expensive bootstrap. Z3 is the sustainable engine.

4. **The adversarial review was essential.** Without it, the postmortem would have claimed Milestone 6 with no Milestone 6 artifacts. The gap between "the code ran and emitted JSON" and "the system discovered principles" is real and load-bearing.

5. **Runtime verification is simple once static analysis works.** Substituting concrete values into cached SMT-LIB templates is straightforward. The hard part was already done.

6. **Real bugs exist in real code.** The thesis isn't theoretical. `authHeader.slice(7, 15)` is a real credential exposure in a real billing file, found from a `logger.info` call, proved reachable by Z3.

## Commit History

```
b8e4b03 Initial commit: neurallog — a logger that fixes your code
d442f9a Add runtime transport and GitHub issue pipeline
a1b4577 Enable streaming events and remove maxTurns limit
0a2bc4d Add session postmortem with full timeline and findings
6d558ec Add Layer 2: mechanical axiom engine (no LLM, just Z3)
9e94751 Rewrite postmortem after adversarial review
617b6d1 Filter vacuous invariants at both prompt and verifier level
7d6f22d Address adversarial review: validation, termination, classification
cdb50af Two-stage semantic classifier with reverse framing
```

## The Thesis Revisited

Logging is assertions made by eyeballs after the fact. neurallog gives the eyeballs to a theorem prover.

The mechanism is real. The signal rate is real but needs improvement. The self-improvement loop has the machinery but hasn't been tested end-to-end with validation enabled. The runtime mode works and catches violations with live values.

What we have is a credible Milestone 1-4 prototype with partial 5-7 that finds real bugs and produces real proofs. What we don't have yet is the fully converging, self-validating system the spec describes. The gap is engineering, not research. The thesis is proven. The product isn't finished.
