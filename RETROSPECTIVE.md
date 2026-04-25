# ProveKit Retrospective

Honest account of what the dogfood sprint built, what it proved, and what is left.

## What landed

The 23-task plan (`docs/plans/2026-04-23-fix-loop.md`) ran through D4 plus follow-ups, closing with the rename commit (`eff60b0`: neurallog to provekit, tagline finalized).

Key milestones in commit order:

- **C2** (commit `5558873`): SAST overlay via scratch git worktree and scratch DB.
- **C3** (commit `8c3cbab`): fix candidate generator with oracle 2 (Z3 under overlay).
- **C4** (commit `62e03bb`): complementary-change generator with oracle 3 per adjacent site.
- **C5** (commit `0f5a8bd`): regression test with mutation verification (oracle 9).
- **C6** (commit `a3fc699`): principle and capability candidate generator with oracles 6, 14, 16, 17, 18.
- **D1a** (commit `4a9dcc4`): bundle persistence schema and artifact-kind registry (fourth primitive).
- **D1b** (commit `ca58b17`): bundle coherence oracle runner, 9 new oracles plus audit-trail verification of 9 already-fired.
- **D2** (commit `881c55e`): transactional apply with substrate rollback, oracle-gated.
- **D3** (commit `6a47bef`): learning layer, library updates, pending fixes, capability registration post-apply.
- **D4** (commit `2ed84f8`): end-to-end acceptance test, division-by-zero happy path and substrate-path routing.
- **Oracle 16 full execution** (commit `d1e5d80`): extractor runs via `typescript.transpileModule` against a scratch DB; no longer a stub.
- **Capture-the-change C3-C6** (commits `0cb7689`, `567f7c2`): agent path plus JSON fallback across all generation stages.
- **Artifact-kind expansion** (commit `e0e1268`): registry expanded to cover non-code fix kinds (test, config, doc, dep, prompt, lint, migration).
- **Dogfood: empty-catch gap** (commit `4a3b103`): substrate-extension path closes end-to-end via `try_catch_block` capability.
- **Rename** (commit `eff60b0`): project renamed to ProveKit.

Test files in `src/fix/`: 17 (each stage has a unit test; the dogfood test covers end-to-end). Total commits: 24.

## What the dogfood proved

### Substrate path (stub LLM)

The concrete proof is `src/fix/dogfood.empty-catch.test.ts`.

The test exercises the full substrate-extension path using the `empty-catch` capability gap. The fixture is a TypeScript function whose catch block is empty. The stub LLM provides canned responses that mimic what a real LLM would produce. Two cases:

**Case 1 (happy path).** The loop runs intake through D1. C6 determines that no existing capability can express "catch handler is empty," proposes a `try_catch_block` capability with a `handler_stmt_count` column, and passes oracles 14, 16, 17, 18. The assembled bundle has `bundleType: "substrate"`, `coherence.migrationSafe: true`, `coherence.extractorCoverage: true`, `coherence.substrateConsistency: true`, `coherence.principleNeedsCapability: true`. The audit trail shows C1 through D1 all completing.

**Case 2 (safety gate).** The stub LLM proposes a migration that contains `DROP TABLE IF EXISTS`. Oracle 14 rejects it. The bundle is either null or `bundleType: "fix"` (not `"substrate"`). A substrate bundle with a destructive migration is never assembled.

Both cases pass.

### First real-LLM closed loop (2026-04-24)

On 2026-04-24, the full pipeline ran end-to-end on a real bug using the Claude Agent SDK with Opus 4.7 as the default model throughout. The input was `dogfood-scratch/divide.ts` (`function divide(a, b) { return a / b; }`) plus a prose bug report. All stages ran: Intake through D2.

Stage log:

```
C1 ✅ formulateInvariant     (Z3 oracle #1 PASS, 12s)
C2 ✅ openOverlay             (scratch worktree, 0.6s)
C3 ✅ generateFixCandidate    (Read + Edit, oracle #2 PASS via path-condition extraction, 20s)
C4 ✅ generateComplementary   (Read + Edit; pruned by oracle #3)
C5 ✅ generateRegressionTest  (Write + Edit; mutation-verified)
C6 ✅ generatePrincipleCandidate
D1 ✅ assembleBundle
D2 ✅ applyBundle (prDraft mode)
```

The patch was a clean guard. The regression test encoded the Z3 witness directly (`b = 0, a = 1`), tolerated either fix shape, and exhaustively rejected `Infinity`, `NaN`, and `-Infinity`. The PR body was auto-generated and written to disk.

The dogfood surfaced five integration gaps that stub-LLM tests could not reach. All five were closed:

1. `provekit analyze` was not populating the SAST tables the fix loop queries. Closed by wiring `buildSASTForFile` into the analyze per-file walk.
2. LLM JSON responses were wrapped in markdown code-fences; `JSON.parse` failed at every call site. Closed by a shared `parseJsonFromLlm()` helper that strips fences. Ten call sites updated.
3. The CLI bridge (`cli.fix.ts`) forwarded `complete()` but not `agent()`. Every C-stage's `if (llm.agent)` dispatch fell back to JSON even though the Claude Agent SDK's `agent()` was wired. Closed by adding the conditional `agent()` forward.
4. Oracle #2 was a proxy (re-evaluate principle, reject if matches remain). Guard-based fixes still match the division pattern after the guard lands; the proxy rejected correct fixes. Closed by path-condition extraction + augmented Z3 (see ARCHITECTURE.md).
5. The overlay was not enforced at the tool level. Prompts containing absolute paths let Claude edit files outside the worktree via Edit directly. Closed by sanitized prompts (overlay-relative paths only) and post-validation throwing `OverlayBypassError` on any path that escapes.

### Pitch-leak closures

`docs/plans/2026-04-25-pitch-leaks.md` named six honest cracks. Three are closed.

**Leak 1 (invariant fidelity).** Oracle 1.5 lands cross-LLM derivation agreement, prose-to-clause traceability, and adversarial-fixture pre-validation, with adaptive routing for taint-style versus arithmetic invariants. Commits `9954876`, `694d731`, `4f344ab`, `d35df4d`. Closes the "Z3 proves the patch satisfies an invariant the LLM wrote from prose" exposure.

**Leak 4 (loop seams).** Deliberate fuzzing surfaced the rest of the integration gaps: 211-scenario corpus across fast-check, SemGrep, Stryker, and a BugsJS skeleton. Integration-gap rate at 0%. Commits `20bf9a2`, `5f3a933`, `0d1f373`, `5d5826d`, `0480c18`. Closes the "five gaps in one run" critique with a corpus-driven hardening pass instead of a single happy path.

**Leak 2 (hard-bug existence proof).** Closed via real-LLM end-to-end on shell-injection. Opus produced both a `taintSource` capability proposal and a `no_unsanitized_shell_exec` DSL principle, and in a separate run auto-applied an `execFileSync` argv-form fix with a regression test. Required chained data-flow via transitive closure (commit `3ce6f4c`) as the substrate prereq, then the substrate-extension dogfood test in commit `20e3257`. The systemic JSON refactor (commit `ad89d5b`, LLM writes JSON to disk via the Write tool, the pipeline reads it) eliminated the prose-prefix and fence-wrapping failure mode that was blocking longer LLM outputs. Closes the "demos are easy mode" critique.

Three leaks remain open: semantic generalization (Leak 3), tier-calibrated speed (Leak 6), and the Leak 5 wording leak which is closed by this document.

## Remaining A8 capability gaps

From `docs/plans/2026-04-23-fix-loop/capability-gaps.md`. These are the bug classes that cannot yet be expressed in the ProveKit DSL because required capabilities or relations are missing. Each is dogfood fuel for the next sprint.

| Gap | Status | What it needs |
|-----|--------|---------------|
| shell-injection | **Done** | `taintSource` capability + `no_unsanitized_shell_exec` DSL principle (proposed by real-LLM substrate run); chained data-flow via transitive closure (commit `3ce6f4c`) was the substrate prereq |
| empty-catch | **Done** | `try_catch_block` capability (landed via substrate dogfood) |
| guard-narrowing | **Done** | `same_value` relation (#66) + parser opens for arbitrary relation names (#67), varDeref in target position (#68), explicit `where RELATION(LHS, RHS)` syntax (#69) |
| loop-accumulator-overflow | Open | `encloses($outer, $inner)` relation (AST ancestor check) |
| param-mutation | Open | `mutates_param` capability column or `assigns_to_param` relation |
| switch-no-default | Open | `has_default` column on `switches` capability |
| ternary-branch-collapse | Open | `literal_value` column on `branches` capability |
| variable-staleness | Open | `last_assigned_line` column + `data_flow_reaches` relation |
| while-loop-termination | Open | Loop termination analysis capability |

Each gap closed produces a substrate bundle that lands a new capability in the registry. The next analysis run can then detect that bug class across the full codebase.

## All 18 oracles implemented

The three formerly-stubbed oracles now have real implementations:

**Oracle 7 (witness replay)** runs the Z3 witness against original and fixed code through the harness in `src/harness.ts`. Original must trigger the bug, fixed must not. Landed in commit `bf46f30`.

**Oracle 12 (DSL no-regressions)** re-runs every existing DSL principle against the overlay SAST and rejects bundles that flip any verdict from no-violation to violation. Landed in commit `bf46f30`.

**Oracle 15 (cross-codebase regression, substrate only)** runs the new capability extractor plus every existing principle against the fixture corpus and rejects bundles that introduce false positives. Landed in commit `5dac896`.

Oracle 1.5 (invariant fidelity, fires inside C1) also landed: cross-LLM derivation agreement, prose-to-clause traceability, adversarial-fixture pre-validation, with adaptive routing that distinguishes abstract taint-style invariants from concrete arithmetic invariants. See `src/fix/invariantFidelity.ts`.

## Known integration gaps

The artifact-kind registry (`src/fix/artifactKindRegistry.ts`) expanded in commit `e0e1268` to cover `test_fix`, `config_update`, `dependency_update`, `documentation`, `lint_rule`, `migration_fix`, `observability_hook`, `prompt_update`, and `startup_assert`. The D1b oracle routing uses `code_patch.isPresent` as an early filter; `code_patch` overlaps with more-specific kinds (`test_fix`, etc.) in some configurations, making the oracle union at D1b over-inclusive for mixed-kind bundles. Flagged during issue 58. A follow-up task is needed to tighten D1b routing per kind.

## What is deferred

The real-LLM loop is closed. Remaining work is engineering toward production:

1. **C5 robustness for arbitrary projects.** The vitest-in-overlay path symlinks the user's `node_modules` into the scratch worktree. Real projects have monorepo workspaces, custom test runners, and varied module systems. Each shape needs explicit handling.

2. **`autoApply` end-to-end.** Every dogfood run used `prDraft` mode (writes patch + PR body to disk). The cherry-pick path with substrate rollback needs end-to-end testing against a clean target branch.

3. **Remaining A8 capability gaps.** Three closed (shell-injection via real-LLM substrate run, empty-catch via stub-LLM substrate dogfood, guard-narrowing via #66-69). The remainder (loop-accumulator, param-mutation, switch-no-default, ternary-branch-collapse, variable-staleness, while-loop-termination) are queued as substrate bundles.

4. **Multi-language support.** Currently TypeScript-only via ts-morph. Tree-sitter wiring exists for the analyze layer; the fix-loop's SAST builder is TS-specific.

5. **Concurrency.** Single-user, single-process, single-worktree. The capability registry is a process-local Map. Multi-user setups need either a shared registry or per-tenant isolation.

6. **Performance.** Each dogfood run takes 2-8 minutes on Opus 4.7 (LLM-latency-bound). Tier-calibrated model selection (haiku for intake parsing, sonnet for classification, opus for invariant formulation) is the documented production path; Opus 4.7 across every stage is the conservative current default.

## What the architecture is ready for

The gap list is a work queue. Each item that closes:

1. Lands a new capability in the registry.
2. Lands a new DSL principle in the library.
3. Extends the substrate so every future analysis can detect that bug class.
4. Proves the architecture can handle a new class of structural complexity.

The system gets strictly smarter with every bundle applied. That is the intended invariant.

The default model is Opus 4.7 (`claude-opus-4-7`). Intake adapters and the CLI bridge default to it. Tiered selection per stage (cheaper model for intake/classify, Opus for invariant formulation) is a deferred optimization.
