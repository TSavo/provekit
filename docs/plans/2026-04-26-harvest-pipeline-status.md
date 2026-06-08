# BugsJS Harvest Pipeline: Status (2026-04-26)

**Operational record for the harvest pipeline build.** Phase 1, 2-A, 2-B, and Phase 4 (lite) shipped this session. End-to-end smoke validated on a real BugsJS bug.

## Architecture (current)

```
HarvestCandidate (Phase 1: extractBugs)
  ↓
Recognition gate (Phase 2-A: recognize.ts, locus-constrained)
  ↓ if recognized: append provenance to library entry, skip discovery
  ↓ if unrecognized:
Discovery (Phase 2-B: discover.ts → tryExistingCapabilities)
  ↓ writes to .sugar/harvest/staging/<project>-bug-<id>.json
  ↓
Promotion (Phase 4-lite: promote.ts)
  ↓ validate.ts: positive (matches own bug) + negative (cohort match rate ≤ 30%)
  ↓ pass → write .dsl + .json to .sugar/principles/, merge provenance
  ↓ fail → quarantine in staging with audit trail
```

## Empirical (2026-04-26 morning run)

| Phase | Number |
|-------|--------|
| BugsJS forks cloned | 8 of 10 (mocha + node-redis renamed at BugsJS org) |
| Bug pairs enumerated | 417 |
| Phase 1 extraction yield | 403 / 417 = **96.6%** (filters: ≤2 files, ≤50 LOC, no test-only) |
| Phase 2-A recognition | 238 / 403 = **59.1%** covered by existing 6-principle library |
| Phase 2-B discovery (smoke) | 1 candidate run end-to-end, 2 shapes emitted |
| Phase 4 promotion | wired and integration-tested (no live promotions yet) |

## Per-project recognition coverage

| Project | Recognized | Total | % |
|---------|-----------|-------|---|
| bower | 3 | 3 | 100% |
| eslint | 182 | 326 | 56% |
| express | 17 | 27 | 63% |
| hessian.js | 7 | 8 | 88% |
| hexo | 9 | 11 | 82% |
| karma | 15 | 19 | 79% |
| pencilblue | 3 | 6 | 50% |
| shields | 2 | 3 | 67% |

## Principle hits (real production bugs at locus)

| Principle | Hits across 403 |
|-----------|----------------|
| falsy-default | 161 |
| addition-overflow | 88 |
| subtraction-underflow | 51 |
| throw-uncaught | 11 |
| multiplication-overflow | 11 |
| empty-collection-loop | 3 |
| unguarded-await | 1 |
| division-by-zero | 1 |

The 6-principle library is already earning its keep on real bugs.

## Validation: locus constraint matters

A naive "principle matches anywhere in any changed file" recognizer reports 100% coverage. With the locus constraint (`@@ -<start>,<len>` from the unified diff, ±3-line neighborhood for ts-morph node-to-source slack), express drops from spurious 100% to a believable 63%. The remaining 161 falsy-default hits ARE within the diff's hunk range; this is meaningful as evidence even if a fraction are syntactic-shape false positives.

## Discovery quality on the same input is variable

Two consecutive smoke runs on express-bug-1 (the Allow-header dedup bug):
- Run 1: produced `ArrayPushWithoutDedupGuard` + `ArrayConcatWithoutDedupGuard` using `same_value` cross-references. Both passed adversarial validation.
- Run 2: produced `ArrayPushWithoutConditionalGuard` + `ArrayPushApplyWithoutFilterGuard` without `same_value` constraints. Both rejected at 3/3 false-positive.

The adversarial validator correctly rejects the broader run-2 shapes; the gate is doing its job. But this is real LLM variance on the same prompt. The same risk class as the Bug-1 v9-v22 chase yesterday: fix-loop infrastructure can be perfected, principle-distillation quality varies per call.

For the harvest pipeline this is OK: rejected discoveries don't pollute the library. The Phase 4 validation gate is mechanical.

## Cost note

End-to-end discovery takes ~11 min per candidate (sonnet invariant synthesis + sonnet C6 proposer + haiku adversarial × N shapes). At 165 unrecognized candidates × 11 min = **~30 hours wall-time sequential.** Parallelizing 4-6 concurrent calls drops this to ~5-8 hours.

## Tests

42 harvest tests across 7 modules:
- extractBugs.test.ts (8): Bug-N..Bug-N-fix tag walking + filters
- recognize.test.ts (10): locus-constrained matching, parseDiffDirtyLines
- provenance.test.ts (6): appendHarvestProvenance idempotence
- synthesize.test.ts (8): BugSignal + FixCandidate synthesis
- discover.test.ts (3): discoverPrinciple end-to-end with stub LLM
- validate.test.ts (4): positive + negative cohort validation
- promote.test.ts (3): staging→library promotion with merge

463 total fix-loop tests passing.

## Commits this session (8 total)

```
d8ffb09  Phase 1 extractor: Bug-N..Bug-N-fix → HarvestCandidate
3b1be40  Phase 2-A recognition (locus-constrained, no LLM)
03e3fd3  Provenance writeback (idempotent, batched)
f4566cf  Phase 2-B discovery: first real principle from a real diff
b95f90e  gitignore staging output
7b1c30f  Provenance writeback wired + Phase 4 staging→library promotion
```

## What's left for #97

- **Calibration on 30-bug subset** per the original spec. Not run yet beyond the single express-bug-1 smoke. Would consume ~5 hours sequential.
- **Phase 5 continuous wiring**: same pipeline plumbed into customer fix loops so every closed bug feeds the harvest. Not started.
- **Discovery parallelism**: current script processes serially. Parallelizing across the LLM provider would cut wall-time materially.
- **Tighter principle library**: the existing 6 entries (falsy-default, addition-overflow, etc.) match too broadly. Refining their DSL with proper `require no` predicates would reduce spurious recognition without changing harvest infrastructure.

The pipeline itself is functional and tested. The remaining work is calibration runs + parallelism + library refinement, not architecture.
