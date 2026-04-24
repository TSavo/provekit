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

The concrete proof is `src/fix/dogfood.empty-catch.test.ts`.

The test exercises the full substrate-extension path using the `empty-catch` capability gap. The fixture is a TypeScript function whose catch block is empty. The stub LLM provides canned responses that mimic what a real LLM would produce. The test has two cases:

**Case 1 (happy path).** The loop runs intake through D1. C6 determines that no existing capability can express "catch handler is empty," proposes a `try_catch_block` capability with a `handler_stmt_count` column, and passes oracles 14, 16, 17, 18. The assembled bundle has `bundleType: "substrate"`, `coherence.migrationSafe: true`, `coherence.extractorCoverage: true`, `coherence.substrateConsistency: true`, `coherence.principleNeedsCapability: true`. The audit trail shows C1 through D1 all completing.

**Case 2 (safety gate).** The stub LLM proposes a migration that contains `DROP TABLE IF EXISTS`. Oracle 14 rejects it. The bundle is either null or `bundleType: "fix"` (not `"substrate"`). A substrate bundle with a destructive migration is never assembled.

Both cases pass. The architecture held under first real-LLM-style load (stub, but with realistic prompt matching and full code execution paths).

## Eight remaining A8 capability gaps

From `docs/plans/2026-04-23-fix-loop/capability-gaps.md`. These are the bug classes that cannot yet be expressed in the ProveKit DSL because required capabilities or relations are missing. Each is dogfood fuel for the next sprint.

| Gap | Status | What it needs |
|-----|--------|---------------|
| shell-injection | Open | `string_composition.has_interpolation` capability + `data_flow_reaches` relation |
| empty-catch | **Done** | `try_catch_block` capability (landed via dogfood) |
| guard-narrowing | Open | `always_exits` relation or `consequent_always_exits` column on `decides` |
| loop-accumulator-overflow | Open | `encloses($outer, $inner)` relation (AST ancestor check) |
| param-mutation | Open | `mutates_param` capability column or `assigns_to_param` relation |
| switch-no-default | Open | `has_default` column on `switches` capability |
| ternary-branch-collapse | Open | `literal_value` column on `branches` capability |
| variable-staleness | Open | `last_assigned_line` column + `data_flow_reaches` relation |
| while-loop-termination | Open | Loop termination analysis capability |

Each gap closed produces a substrate bundle that lands a new capability in the registry. The next analysis run can then detect that bug class across the full codebase.

## Known MVP pass-throughs

Three oracles are documented stubs, not full implementations:

**Oracle 7 (witness replay).** Currently a pass-through returning `passed: true`. Full implementation requires runtime harness wiring: the Z3-witness-derived inputs need to be executed against the fixed code in the overlay to confirm they trigger the bug on original code and do not trigger it on the fixed code. The harness infrastructure (`src/harness.ts`) exists. Wiring it into the oracle is deferred.

**Oracle 12 (DSL no-regressions).** Currently a pass-through. Full implementation would re-run all existing DSL principles against the overlay's SAST to confirm none flip from no-violation to violation. Deferred to D3 post-apply.

**Oracle 15 (cross-codebase regression, substrate only).** Currently a pass-through. Full implementation would run the new capability extractor and principle against a fixture corpus of known-clean codebases to confirm no false positives are introduced. Deferred to D3 fixture corpus.

All three are documented in `src/fix/oracles.ts` with inline rationale.

## Known integration gaps

The artifact-kind registry (`src/fix/artifactKindRegistry.ts`) expanded in commit `e0e1268` to cover `test_fix`, `config_update`, `dependency_update`, `documentation`, `lint_rule`, `migration_fix`, `observability_hook`, `prompt_update`, and `startup_assert`. The D1b oracle routing uses `code_patch.isPresent` as an early filter; `code_patch` overlaps with more-specific kinds (`test_fix`, etc.) in some configurations, making the oracle union at D1b over-inclusive for mixed-kind bundles. Flagged during issue 58. A follow-up task is needed to tighten D1b routing per kind.

## What is deferred

**Real-LLM dogfood.** Everything in the system uses stub LLMs for testing. The `ClaudeAgentProvider` (`src/llm/ClaudeAgentProvider.ts`) is fully implemented and wires into the `@anthropic-ai/claude-agent-sdk`. What is missing is environment configuration: setting `ANTHROPIC_API_KEY` and pointing the provider factory at Claude. Not hard, just not done. The `ProviderFactory` and `ProviderPool` in `src/llm/` are ready.

Once a real LLM is wired, the eight remaining gaps become runnable dogfood: submit a bug report for each gap, watch C6 propose a new capability, verify oracles 14-18 pass, apply the substrate bundle, and confirm the new capability detects the pattern across the full codebase.

## What the architecture is ready for

The gap list is a work queue. Each item that closes:

1. Lands a new capability in the registry.
2. Lands a new DSL principle in the library.
3. Extends the substrate so every future analysis can detect that bug class.
4. Proves the architecture can handle a new class of structural complexity.

The system gets strictly smarter with every bundle applied. That is the intended invariant.
