# ProveKit Production-Readiness Plan

**Goal:** move ProveKit from "the loop demonstrably closes on a real LLM in a controlled scratch project" to "any TypeScript project can run `provekit fix` against a real bug and trust the result."

**Architecture:** every primitive is a registry, every LLM output passes a mechanical oracle. Production-readiness is not new architecture; it's hardening, breadth, and operability.

**Sequencing principle:** Tier 1 items are sequential because each unblocks user feedback that informs the next. Tier 2 and beyond are largely parallel; the first task in each tier validates the pattern and the rest follow.

---

## Tier 1 — Blocks real use (sequential)

### Task P1: C5 robustness for arbitrary projects

**Why:** Today's `runTestInOverlay` assumes the user's project has `node_modules/vitest` at the project root. Real projects are: jest, mocha, node:test, monorepos with workspaces, projects with no test runner, projects where vitest is in a workspace package not the root. Each of these breaks oracle #9 in C5 immediately. The first thing a real user encounters when they try `provekit fix` against their codebase.

**Files to touch:**
- Modify: `src/fix/testGen.ts` (`runTestInOverlay`, `resolveMainRepoRoot`)
- Create: `src/fix/testRunners/index.ts` — runner registry (sixth primitive registry)
- Create: `src/fix/testRunners/{vitest,jest,mocha,nodetest,none}.ts` — per-runner adapters

**Approach:**

A test-runner registry parallel to the other five. Each adapter is a descriptor:

```ts
interface TestRunnerDescriptor {
  name: string;
  detect: (projectRoot: string) => boolean;
  resolveRunnerBinary: (projectRoot: string, overlay: OverlayHandle) => string;
  invocation: (testFilePath: string) => string[];
  parseOutcome: (exitCode: number, stdout: string, stderr: string) => { passed: boolean; testCount: number; details: string };
}

registerTestRunner({...});  // one per runner
```

Detection priority:
1. Check `package.json.scripts.test` and parse the runner from it
2. Check for vitest.config.ts → vitest, jest.config.js → jest, etc.
3. Walk up to find a workspace root (pnpm-workspace.yaml, lerna.json, npm workspaces) and recheck there
4. Fall through to "no runner detected" → oracle #9 returns informational pass with detail "no test runner; mutation verification skipped"

For monorepo node_modules resolution: use Node's module-resolution algorithm starting from the locus's directory, walking upward for `node_modules` until found. The existing realpath-symlink fix from C5 lands close enough; finalize.

**Tests:** in-memory git repos with each runner's config shape. Assert correct adapter selected. Real spawn for a tiny vitest fixture + a tiny jest fixture (mark as `.slow` if needed).

**Complexity:** 4-5 task agents. One per runner adapter. Parallelizable after the registry shape lands.

**Risk:** medium. Each runner has surface differences in stdout format. Robust parsing requires real fixtures.

---

### Task P2: autoApply end-to-end with real LLM

**Why:** Every real-LLM dogfood ran in prDraft mode (writes patch + PR body). The cherry-pick path (`--apply`) has unit-test coverage but never ran with a real Claude-produced bundle. The transactional rollback path (substrate-bundle migration revert on failure) has unit-test coverage and zero real runs. This is the "I trust this enough to commit" gate.

**Files to touch:**
- No code changes expected up-front; this task is verification-driven.
- Likely findings: 1-3 small fixes in `src/fix/apply.ts`.

**Approach:**

1. Reset the dogfood-scratch fixture to its buggy state.
2. Run `provekit fix bug-report.md --apply --verbose`.
3. Watch the apply path:
   - Cherry-pick should succeed onto the target branch.
   - The target branch's HEAD should now contain the fix + the regression test.
   - The main DB's `fix_bundles` row should have `applied_at` and `commit_sha` populated.
4. For substrate bundles: run a separate scratch with one of the A8 capability gaps (e.g., empty-catch). Use `--apply`. Verify migration applies, capability files persist, principle library row inserts.
5. For rollback: deliberately introduce a fault late in apply (e.g., interrupt via `kill`). Verify migration rollback runs cleanly.

Document findings as commits. Most likely: edge cases around merge conflicts, stale worktree state, or principle library write ordering.

**Tests:** end-to-end test in `src/e2eApply.test.ts` (new) using StubLLM to fake the bundle artifacts but real git operations.

**Complexity:** 2-3 task agents. Drives a few small `apply.ts` fixes.

**Risk:** medium. Git operations are well-behaved; the risk is in unanticipated state interactions (worktree cleanup races, branch tracking, etc.).

---

### Task P3: Real-LLM dogfood of the substrate-extension path

**Why:** The empty-catch test in `src/fix/dogfood.empty-catch.test.ts` closes the substrate path with StubLLM responses that simulate a well-behaved Claude. Real Claude has never proposed a CapabilitySpec end-to-end. The entire substrate-self-extension thesis hinges on this working with a real model. If real-LLM substrate proposals are flaky in ways stubs can't expose, that's a research finding, not engineering, and we want to know now.

**Files to touch:**
- No initial code changes; verification-driven.
- Likely findings: prompt clarity issues in `capabilityGen.ts`, oracle calibration for real-LLM CapabilitySpecs.

**Approach:**

1. Pick one A8 capability gap (suggested: `encloses` for loop-accumulator-overflow — simple semantics, unlocks 5 principles per the memo).
2. Build a scratch project with a fixture exhibiting the bug pattern.
3. Write a bug report.
4. Run `provekit fix bug-report.md --no-confirm --verbose`.
5. Watch C6 route to `proposeWithCapability`. The full-transcript log captures Claude's reasoning + the proposed CapabilitySpec.
6. Verify each substrate oracle fires correctly:
   - #14 (migration safety): SQL is non-destructive
   - #16 (extractor coverage): full execution against fixtures
   - #17 (substrate consistency): schema FKs valid
   - #18 (principle-needs-capability): without/with capability compile dance
   - #15 (cross-codebase regression): per-principle, per-file verdict comparison

Each oracle that flakes or false-rejects is a real-LLM dogfood finding. Document; close.

**Tests:** the dogfood IS the test. After it passes, codify as a real-LLM equivalent of `dogfood.empty-catch.test.ts` (skipped by default since it costs LLM tokens; runnable on demand).

**Complexity:** 1 task to set up + run, then 1-3 follow-up tasks per finding.

**Risk:** highest in this plan. This is where research-not-engineering surprises can show up. If they do, surface them honestly; the architecture either scales or it tells us what's missing.

---

## Tier 2 — Quality, breadth, operability (parallel)

### Task P4: Tiered model selection

**Why:** Opus 4.7 across every stage costs 5-10 minutes per loop. Production UX needs cheap models on cheap stages. Intake parsing doesn't need Opus; invariant formulation does. Mismatch costs nothing today (single dogfood); costs a lot at scale.

**Files to touch:**
- Modify: `src/cli.fix.ts` (bridge defaults can stay Opus as a per-stage override)
- Modify: `src/fix/orchestrator.ts` (pass per-stage model selection through)
- Modify: each stage that calls the LLM, accept an optional `model` arg from a config map
- Add: `src/fix/modelTiers.ts` — config: per-stage default model, env-overridable

**Approach:**

```ts
// src/fix/modelTiers.ts
export const DEFAULT_MODEL_TIERS = {
  intake: "haiku",        // structured-output extraction; fast tier sufficient
  classify: "sonnet",     // multi-class judgment; mid-tier
  C1: "opus",             // invariant formulation; load-bearing, opus
  C3: "sonnet",           // fix candidate via agent; sonnet handles tool use well
  C4: "sonnet",
  C5: "sonnet",
  C6: "opus",             // principle/capability proposal; load-bearing, opus
  adversarial: "haiku",   // adversarial validation generates many fixtures; haiku
};
```

Override via `PROVEKIT_MODEL_<STAGE>=opus` env vars or `--model-tier <stage>=<tier>` CLI repeat flags.

**Tests:** unit tests asserting each stage receives the expected tier given a config map. Integration test: spy on the LLMProvider stub to verify per-stage `model` arg.

**Complexity:** 1 task agent.

**Risk:** low. Already proven each tier works with the existing stub LLM. Concrete tuning happens after the first real-LLM run shows where haiku is too dumb (likely classify, possibly intake).

---

### Task P5: Close the 7 remaining A8 capability gaps via real-LLM substrate bundles

**Why:** Each gap closed = one more class of bug the principle library handles for any codebase forever. Compounding asset. Per the A8 memo, `encloses` alone unlocks 5 principles. The other six gaps (string_composition for shell-injection, control-flow capabilities for guard-narrowing, etc.) close their own clusters.

**Files to touch:**
- Each gap closure produces a substrate-bundle commit modifying:
  - `src/sast/schema/capabilities/<name>.ts` (new schema file)
  - `src/sast/capabilities/extractor.ts` (or new file) — the new extractor
  - A new drizzle migration
  - One or more `.provekit/principles/<name>.dsl` migrations to use the new capability

**Approach:**

Sequence by complexity per the A8 memo:
1. `encloses` (relation, not capability — simpler)
2. Add structural capabilities for try_catch_block (already done as the empty-catch dogfood proof — skip)
3. `has_default` column on decides for switch-no-default
4. `literal_value` for ternary-branch-collapse
5. Always-exits relation for guard-narrowing
6. `data_flow_same_value` (already done as same_value relation #66 — skip)
7. String composition + tainted-flow for shell-injection
8. Liveness analysis for variable-staleness (hardest)
9. Termination analysis for while-loop-termination (hard)

Per gap: write a bug-report fixture, run `provekit fix`, watch C6 propose the CapabilitySpec, verify oracles, apply. Each closure is its own substrate bundle — autonomous through the loop.

**Tests:** each gap's closure includes an updated equivalence test in `src/pipeline/DerivationPhase.dslEquivalence.test.ts` that asserts the previously-gap principle now migrates cleanly.

**Complexity:** 7 task agents (one per remaining gap). Parallelizable after the first 2-3 prove the substrate-bundle pattern holds with real Claude.

**Risk:** medium. Hard gaps (liveness, termination) may surface deeper architectural questions than substrate extension can solve mechanically.

---

### Task P6: Operator CLI surface

**Why:** The system runs but isn't operable. Bundles get persisted to the DB, audit trails accumulate, principle libraries grow — and there's no way for a human to inspect any of it. `provekit fix` is the input surface; we lack the output surface.

**Files to touch:**
- Create: `src/cli/review.ts` — `provekit review <bundle-id>` walks audit trail
- Create: `src/cli/pending.ts` — `provekit pending` lists pending_fixes queue
- Create: `src/cli/promote.ts` — `provekit promote <principle> --tier warning` updates confidence_tier
- Create: `src/cli/principles.ts` — `provekit principles list` enumerates the library
- Modify: `src/cli.ts` register new subcommands

**Approach:**

Each subcommand is a thin DB-read with formatted output.

`provekit review <bundle-id>`:
- Load fix_bundles row + all fix_bundle_artifacts + all llm_calls + the audit_trail JSON
- Pretty-print: bundle metadata, per-stage timeline, oracle verdicts, full LLM calls (count + char totals; --verbose shows full prompts/responses from the .provekit/fix-loop-<ts>.log)
- Highlight any oracle that ran but returned passed=false (rejected sites)
- Output mode: `--json` for machine consumption, default human-readable

`provekit pending [--principle <name>] [--limit N]`:
- Query pending_fixes ordered by priority
- One-line-per-row: source bundle, site file:line, reason

`provekit promote <principle-name> --tier {advisory|warning|blocking}`:
- Update principles_library row
- Confirm with audit log entry
- Refuse to demote without `--force`

`provekit principles [list|show <name>]`:
- list: every principles_library row + tier + match counts in main DB
- show <name>: full DSL source + JSON descriptor + recent matches

**Tests:** unit tests for each subcommand against in-memory DB.

**Complexity:** 4 task agents (one per subcommand). All independent.

**Risk:** low. Pure read-side CLI work.

---

## Tier 3 — Long-term (deeper architectural)

### Task P7: Multi-language support

**Why:** TS-only via ts-morph today. ProveKit's ambition is broader; the principles + DSL are language-agnostic in spirit. The substrate is what's TS-specific.

**Files to touch:** large surface. Per language:
- Per-language parser adapter (tree-sitter scaffolding exists in analyze)
- Per-language SAST builder
- Per-language capability extractor implementations
- Per-language DSL relation implementations where the SQL doesn't generalize

**Approach:**

Phase 1 (P7a): introduce a language registry. Each language descriptor names its parser, its SAST-build function, its extractor wiring. Refactor existing TS code to fit the descriptor shape. No behavioral change.

Phase 2 (P7b): pick a target language to prove the pattern. Suggested: Python (substantial real-world demand, tree-sitter-python is mature). Implement enough of the substrate to handle 3-5 of the 23 seed principles in Python.

Phase 3 (P7c): scale per-language as demand justifies.

**Tests:** per-language SAST equivalence — building SAST from a Python fixture produces nodes/capabilities/dataflow rows comparable to the TS equivalent for analogous code.

**Complexity:** Phase 1 is 3-4 tasks. Phase 2 is 8-12 tasks. Phase 3 is per-language at similar cost.

**Risk:** highest scope. Each language has its own ergonomic surprises (Python's dynamic typing makes data-flow harder; Go's static typing makes some things easier but adds method-resolution complexity).

---

### Task P8: Concurrency

**Why:** Today: process-local registry Map, single-user assumption, file-based fix_bundles store. Production multi-tenant SaaS needs shared state.

**Files to touch:**
- Modify: every registry module to optionally read from a shared store
- Add: `src/registry/store.ts` — pluggable backend (in-process Map, SQLite-shared, postgres-shared)

**Approach:**

Tier 1: identify the deployment shape. Single-team self-hosted vs. multi-tenant SaaS vs. multi-process single-user. Each shape has a different concurrency model.

Tier 2: for self-hosted multi-process: file-based registry stores + file locks. Each process loads on startup, writes propagate via inotify/fswatcher. Simple, correct, single-machine.

Tier 3: for SaaS multi-tenant: per-tenant registry isolation. Tenant ID threaded through every fix-loop call. Capabilities/principles/etc. scoped to tenant.

**Tests:** concurrency tests using parallel-vitest with shared state.

**Complexity:** Tier 2 is 4-5 tasks. Tier 3 is 8-12 tasks plus tenant lifecycle management.

**Risk:** medium. Concurrency bugs are notoriously hard to test; the architecture's stateless-pipeline design helps.

---

### Task P9: D1b routing dedup for artifact-kind overlap

**Why:** Currently `code_patch.isPresent` and `test_fix.isPresent` both fire when a bundle has a primary fix touching only test files. D1b unions oracle sets across all matching kinds, so the test_fix bundle inherits code_patch's full oracle list. Over-inclusive but not wrong; flagged in #58 review.

**Files to touch:**
- Modify: `src/fix/artifactKindRegistry.ts` — add `priority: number` field
- Modify: `src/fix/bundleAssembly.ts` — when multiple kinds match, take highest priority and skip others (unless explicitly multi-kind)

**Approach:**

```ts
interface ArtifactKindDescriptor {
  // existing fields...
  priority: number;  // higher = more specific, preferred when multiple match
}

// In bundleAssembly, when iterating kinds:
const matched = listArtifactKinds().filter(k => k.isPresent(bundle.artifacts));
matched.sort((a, b) => b.priority - a.priority);
// Take highest priority; skip lower unless they apply to non-overlapping artifacts
```

Set priorities: test_fix=10, config_update=10, doc_update=10 > code_patch=5. The specific kinds win when both match.

**Tests:** unit tests covering each artifact-kind overlap case.

**Complexity:** 1 task agent.

**Risk:** very low. Pure logic refinement.

---

## Total task count + sequencing

| Tier | Tasks | Parallel? | Real-LLM cost |
|---|---|---|---|
| 1 (P1, P2, P3) | ~8 task agents | sequential | medium-high |
| 2 (P4-P6) | ~12 task agents | parallel after first | medium |
| 3 (P7-P9) | ~25-40 task agents | mostly sequential per language/concern | low |

**Recommended pace:**
- Land Tier 1 in next session/work-block (3 sequential dispatches).
- Tier 2 in parallel batches over following sessions.
- Tier 3 prioritized by user demand (multi-language) and operational readiness (concurrency).

**Each tier ends with a milestone we can publish:**
- Tier 1 done = "ProveKit runs on any TypeScript project."
- Tier 2 done = "Closed library compounds across runs; humans can operate it."
- Tier 3 done = "ProveKit scales to multi-language, multi-tenant production."

The architecture doesn't change. Every task in this plan is engineering, not research. The substrate-extension thesis already passed its load test.
