# BugsJS Harvest: Bootstrapping the Principle Library from 452 Real Fixes

**Status:** Spec. Blocked on Leak 3 layer 1 (in flight) + Leak 6 win 2 (in flight).

## The Reframe

BugsJS is not a fuzz-scenario corpus. It is not a benchmark. It is the **bootstrap input for the principle library**.

Each BugsJS entry gives us:
- `Bug-N-original`: production code with the bug
- `Bug-N-fix`: the merged fix
- `Bug-N-test`: the regression test the developer wrote
- Provenance: real project, real commit, real human review

The fix loop's existing C6 already does diff-to-principle abstraction. Today it's called from the fix-loop pipeline (after C1-C5 generate context). The harvest reframe: **C6 is the principle-distillation primitive, callable from any source of (buggy, fixed) pairs.** BugsJS is one such source. Customer fix loops are another.

## The Promise This Underwrites

> "ProveKit learns from bug shapes, code-independent."

Until now this has been demonstrated on synthetic fixtures (division-by-zero, empty-catch). With BugsJS harvested, it has empirical grounding from day one: a principle library validated by ~150-250 real production fixes across 10 mature Node.js projects (Express, Mocha, ESLint, Karma, Hexo, Bower, Hessian.js, node-redis, Pencilblue, Shields).

Every customer's first bug benefits from every prior bug ever closed. The fix loop's expensive path becomes the exception (novel bug classes); the library short-circuit becomes the dominant case.

## Risks (Honest)

### Risk 1: Abstraction-level is hard

A fix that adds `if (b === 0) throw` could be classified as:
- division-by-zero (specific)
- "guard against degenerate arithmetic input" (medium)
- "input validation before use" (general)
- "defensive programming" (too general)

Pick too specific, library bloats with per-bug principles. Pick too general, principle false-positives across the corpus. This is the open problem from Leak 3 layers 2-3 (multi-quarter). The Leak 3 layer 1 work (alternative shapes within one class) handles intra-class variants but does NOT solve cross-bug deduplication (when do two harvests collapse into one principle?). That's a clustering problem on principle structure.

**Mitigation:** the harvest prompt asks the LLM for a `bug_class_id` (slug). Two harvests with the same `bug_class_id` are dedup candidates. A staging area + manual inspection on a 30-bug subset BEFORE committing to the full 452 lets us calibrate.

### Risk 2: Diff cleanliness

Real merged commits include noise:
- Bug fix + drive-by refactor + lint cleanup in one commit
- Workarounds for upstream bugs
- Test-only fixes (no production principle)
- Whitespace + import reorder

Realistic expectations: ~50-70% of BugsJS diffs yield clean harvests. The bootstrap story is **"452 attempts → ~250 valid harvests → ~150 unique principles after dedup."** Still unprecedented; just calibrated.

**Mitigation:** filter at ingest. Skip bugs whose fix touches >2 files or >50 LOC. Skip bugs whose fix is purely under `test/` or `__tests__/`. Log skipped bugs with reason; the count is informational.

### Risk 3: Cross-codebase generalization isn't infinite

10 mature Node.js projects are all server-side JS, mostly Express-adjacent. A principle harvested here may not catch a React/Vue/RN bug. The "code-independent" claim is bounded by the dataset diversity.

**Mitigation:** acknowledge the bound in the marketing claim. "Day one: 100+ real bug classes from 10 production Node.js projects" not "every JavaScript bug ever."

## Sequencing

### Prerequisite: Leak 3 layer 1 (#96, in flight)

C6 must already produce `bug_class_id` and support multiple-shape principles per bundle. Harvest needs `bug_class_id` as the dedup key.

### Prerequisite: Leak 6 win 2 (#95, in flight)

Library short-circuit at C1 must already work — a populated library only matters if the loop can use it.

### Then: harvest pipeline (#97 below)

## Pipeline Design

### Phase 1 — Source extraction

`scripts/clone-bugsjs.sh`: shallow-clones the 10 BugsJS forks into `~/bugsjs/{express,mocha,eslint,karma,bower,hexo,hessian.js,node_redis,pencilblue,shields}`.

`src/fix/harvest/extractBugs.ts`: walks each fork's `Bug-*-original` / `Bug-*-fix` tag pairs, computes diffs, emits a `HarvestCandidate[]`:

```ts
interface HarvestCandidate {
  source: { project: string; bugId: string; tag: string };
  buggyFiles: Record<string, string>;   // path → content at -original
  fixedFiles: Record<string, string>;   // path → content at -fix
  diff: string;                          // unified-diff text
  commitMessage: string;                 // from -fix tag
  testFiles: Record<string, string>;    // from -test tag, for the regression test
}
```

Filters at this stage: skip if `>maxFiles` or `>maxLoc` or test-only changes.

### Phase 2 — Two-mode harvest: recognition then discovery

For each `HarvestCandidate`, the pipeline decides whether to use the LLM or mechanical matching.

**Recognition mode (preferred — no LLM).**

Run the existing principle library against the buggy snapshot. For each principle, check whether it matches at any node in the diff's locus. If a match: the bug is a member of that principle's class.

- Record provenance: append `{ project, bugId }` to the principle's `provenance[]` field.
- If matched at a syntactic shape the principle didn't already enumerate: feed the new shape to Leak-3-layer-1 alternate-shape extension; the principle grows wider without an LLM call.
- Skip discovery mode entirely for this bug. **Cost: ~zero.**

This is the same operation Leak 6 win 2 implements at C1 in production fix loops. The harvest pipeline calls it in batch.

**Discovery mode (fallback — full LLM call).**

Triggered when no existing principle matches the buggy snapshot. The LLM derives a new principle from the diff:

1. Synthesize the `BugSignal` from the commit message (LLM step: "what does this fix prevent?")
2. Synthesize the `InvariantClaim` from the diff (LLM step: "what's the invariant that the fix establishes?")
3. The `FixCandidate` is the diff itself (no LLM needed; we already have it).
4. Call C6's principle-generation directly with these synthesized inputs.
5. Output: `HarvestedPrinciple` = principle DSL + `bug_class_id` + initial provenance + confidence

**Cost compounding.**

Assuming ~100 unique bug classes across 452 BugsJS entries with average cluster size 4-5:
- Discovery mode runs ~100 times (first-of-class)
- Recognition mode runs ~350 times (cluster members)
- Total LLM cost: ~5x lower than naive "LLM per bug"
- Total wall time: similarly compressed

Crucially: this isn't a harvest-specific optimization. It IS the library short-circuit (Leak 6 win 2), batched. The same code path that makes production fix loops fast on known bug classes makes bootstrap cheap. Library size is the leverage point both feed.

### Phase 3 — Staging + dedup

`src/fix/harvest/stagingArea.ts`: writes harvested principles to `.provekit/harvest/staging/<bug_class_id>/<source-id>.dsl` instead of directly into the library.

Dedup pass: group staged principles by `bug_class_id`. For each group:
- If structurally identical (modulo identifiers): merge into one principle, annotate with all source provenances.
- If structurally divergent: leave as separate candidates for manual review.

### Phase 4 — Validation (oracle #15)

For each staged principle, run oracle #15 against the BugsJS source corpus (the buggy + fixed snapshots, used as a flat directory of real production code). A principle that passes:
- Catches the bug it was harvested from (positive validation)
- Doesn't false-positive across the rest of the corpus (negative validation)
gets promoted from `staging/` to `.provekit/principles/`.

A principle that flickers stays in `staging/` with a quarantine flag.

### Phase 5 — Continuous

Beyond bootstrap: every customer fix loop that produces a new principle ALSO goes through the same staging + dedup + validation flow before joining the library. Bootstrap is a one-time bulk-fill; the pipeline is permanent.

## Calibration Plan

Don't run the full 452 first.

1. **30-bug subset.** Pick ~3 bugs per project. Run the harvest pipeline. Manually inspect the principles produced. Calibrate the abstraction-level prompts. Iterate until quality is acceptable on the subset.
2. **Full 452.** Run with the calibrated prompts. Count: how many produced clean harvests, how many were too noisy, how many duplicated existing principles.
3. **Validation report.** "X principles harvested, Y duplicates merged, Z quarantined for review, K caught additional bugs in the corpus that they weren't harvested from." That last number is the empirical "code-independent generalization" claim with real data behind it.

## Acceptance for Spec Sign-Off

The harvest pipeline is "done" when:
- 30-bug subset produces a manually-acceptable principle library (subjective; you'd ship it)
- Full 452 run produces ≥100 unique post-dedup principles, ≥50% pass oracle #15 validation
- Bootstrap can be re-run on demand with a single command (`provekit harvest --bootstrap`)
- Continuous harvest from customer fix loops uses the same staging area and dedup logic
- The principle library, including provenance, is human-readable and auditable per principle

## What This Doesn't Solve

- Cross-codebase outside the BugsJS family (React/Vue/Angular not represented)
- Multi-language harvest (BugsJS is JS-only)
- Bugs whose fix is "rip out and rewrite this whole module" (no harvestable principle)
- Bugs whose fix changes external configuration (no source-file principle)

These are honest scope limits, not failures. They're future workstreams.
