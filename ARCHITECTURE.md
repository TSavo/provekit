# ProveKit Architecture

A guide for understanding the system in 15 minutes. This is not a plan or a spec. It describes what is built, how the pieces relate, and where the interesting design decisions live.

## The thesis

A bug report enters. A mechanically-verified fix bundle exits. The LLM is a participant at every stage boundary, proposing code patches, invariants, regression tests, and substrate extensions. The mechanical oracles are in the hot path, not the review path. They do not evaluate prompts or outputs for quality; they evaluate whether formal properties hold under Z3, whether regression tests pass and fail in the right directions, whether the SAST index is structurally coherent, and whether migrations are safe. When an oracle fails, the pipeline stops. The LLM is fungible. The oracles are not.

## The pipeline

**Intake.** A raw signal arrives: a markdown bug report, a runtime log entry, a gap report ID, or a test failure. An intake adapter normalizes it into a `BugSignal`: summary, failure description, fix hint, code references, and a bug class hint. The adapter is resolved via the intake adapter registry. Today the registry contains four adapters: `report` (markdown), `runtimeLog`, `gapReport`, and `testFailure`.

**Locate.** The normalized signal's code references are resolved against the SAST database to produce a `BugLocus`: the specific AST node, containing function, file, and line where the bug lives. If no node is found, the pipeline stops here.

**Classify.** The signal and locus are presented to the LLM as a classification problem. The LLM selects a primary remediation layer from the registry (`code_invariant`, `config`, `data_ingress`, `observability`, `spec_drift`, `timing`, `infrastructure`, `out_of_scope`) and proposes a list of artifact kinds the fix will need. The output is a `RemediationPlan`.

**C1: Formulate invariant.** The LLM converts the bug description into a formal invariant expressed as an SMT-2 script. Oracle 1 verifies the script is satisfiable (the bug state can be reached) and not vacuously unsat. The invariant becomes the formal contract the fix must satisfy.

**C2: Open overlay.** A scratch git worktree is created from HEAD. A separate SAST database is built for it. All subsequent stages that touch code operate inside this overlay. The main repository and its SAST database are never modified during generation.

**C3: Generate fix candidate.** The LLM proposes code patches inside the overlay. The preferred path uses `agent()` mode: the LLM edits files directly in the overlay worktree, then `git diff` captures the structured change as a `CodePatch`. A JSON-patch fallback exists for providers that do not implement `agent()`. Oracle 2 uses path-condition extraction: `extractGuardConditions()` walks dominance, decides, and node_returns to extract path conditions at the fix site, conjoins them with the original violation SMT, and passes the augmented SMT to Z3. Augmented SMT unsat means the invariant holds under the proposed fix. A proxy-match approach (re-evaluating the principle and checking for remaining matches) was the original implementation but rejected correct guard-based fixes; path-condition extraction replaced it. If oracle 2 fails, the LLM gets one retry with the failure details. If both attempts fail, the pipeline stops.

**C4: Generate complementary changes.** The SAST is queried for adjacent sites with the same bug pattern: callers that need updating, missing guards upstream, observability hooks, startup assertions. Each proposed complementary change is verified by oracle 3 (Z3 re-verification at that site under the overlay).

**C5: Generate regression test.** The LLM generates a vitest regression test from the Z3-witness-derived inputs. Oracle 9 has two halves: the test must pass against the fixed code (9a) and must fail against the original code after the fix is reverted (9b). A test that does not lock in the fix cannot pass oracle 9.

**C6: Generate principle candidate.** The LLM first tries to express the invariant using existing DSL capabilities (`tryExistingCapabilities`). If an existing capability can express it, the LLM produces a `principle` candidate directly. Oracle 6 runs adversarial validation: a second LLM call attempts to construct a counterexample to the principle. If no existing capability can express the invariant, the loop routes to the substrate-extension path (see below). The output is a `PrincipleCandidate`, either plain or with a `CapabilitySpec`.

**D1: Assemble bundle.** All artifacts are collected and the full 18-oracle suite runs. Nine oracles already fired during C1-C6 and are verified from the audit trail. Nine new oracles run here: proven-clause no-regression (4), bundle SMT coherence (5), witness replay (7, MVP pass-through), no-new-gaps (8), full vitest suite (10), SAST structural coherence (11), DSL no-silent-regressions (12, MVP stub), gap closure confirmation (13), and cross-codebase regression (15, substrate only, MVP stub). If all oracles pass, a `FixBundle` is persisted to the database and returned. The bundle type is `"fix"` for normal bundles and `"substrate"` for bundles that include a capability extension.

**D2: Apply bundle.** If `autoApply` is set, the bundle is cherry-picked onto the target branch and the migration (for substrate bundles) is executed transactionally: if the migration fails, the code commit is rolled back. If `prDraftMode` is set (the default when `--apply` is not passed), a unified diff and PR body are written to the working directory for human review.

**D3: Learn from bundle.** After a successful apply, the principles library is updated with the new principle, the capability registry is updated with the new capability (for substrate bundles), and any closed gap reports are marked resolved.

## The five primitive registries

**Capability registry** (`src/sast/capabilityRegistry.ts`). The SAST substrate's vocabulary. Each capability is a SQLite table that the AST extractor populates: node IDs and structured properties extracted from the syntax tree. Today the registry contains capabilities for divisions, function calls, try-catch blocks (added by the empty-catch dogfood), assignments, and others. A new capability lands here when C6 proposes one and oracles 14-18 pass. Extension requires: a schema TypeScript file, a migration SQL file, an extractor TypeScript file, and a registry registration call.

**DSL relation registry** (`src/dsl/relationRegistry.ts`). The relations that the ProveKit DSL can express in `where` clauses: `calls`, `assigns`, `decides`, `iterates`, and others. Each relation maps a DSL predicate to a capability table join. Adding a relation means adding an entry here plus the backing capability. The gap analysis in `docs/plans/2026-04-23-fix-loop/capability-gaps.md` identifies which bug classes need new relations that do not yet exist.

**Intake adapter registry** (`src/fix/intakeRegistry.ts`). Maps signal source strings to adapter functions. Today: `report`, `runtimeLog`, `gapReport`, `testFailure`. Adding a new signal source means registering an adapter here; the pipeline is otherwise unchanged.

**Remediation layer registry** (`src/fix/remediationLayerRegistry.ts`). Maps layer names to layer descriptors that specify which artifact kinds apply and which oracles gate each kind. Today: `code_invariant`, `config`, `data_ingress`, `observability`, `spec_drift`, `timing`, `infrastructure`, `out_of_scope`. Adding a new layer adds a descriptor here; the classifier LLM can then propose it.

**Artifact kind registry** (fifth, per-kind oracle router, `src/fix/artifactKindRegistry.ts`). Maps artifact kind strings (`code_patch`, `regression_test`, `principle_candidate`, `capability_spec`, `complementary_change`, `config_update`, `dependency_update`, `documentation`, `lint_rule`, `migration_fix`, `observability_hook`, `prompt_update`, `startup_assert`, `test_fix`) to oracle sets and assembly logic. D1 routes bundle assembly through this registry; each kind brings its own set of required oracles.

## The 18 oracles

| # | Name | Stage | Verifies |
|---|------|-------|----------|
| 1 | Invariant satisfiability | C1 | SMT-2 invariant is sat (bug is reachable); not vacuously unsat |
| 2 | Fix invariant under overlay | C3 | Z3 negated-goal is unsat under overlay; invariant holds after fix |
| 3 | Complementary site verification | C4 | Invariant holds at each adjacent site under overlay |
| 4 | No proven-clause regression | D1 | All previously-proven clauses in main DB are still unsat |
| 5 | Bundle SMT coherence | D1 | Combined invariant set is satisfiable (no internal contradiction) |
| 6 | Adversarial principle validation | C6 | Adversarial LLM call fails to construct a counterexample to the principle |
| 7 | Witness replay | D1 | (MVP pass-through) Runtime harness re-runs on fixed code; deferred to D2 |
| 8 | No new gaps introduced | D1 | Overlay gap-report count does not exceed main DB count |
| 9 | Regression test two-way | C5 | Test passes on fixed code (9a) and fails on original code after revert (9b) |
| 10 | Full vitest suite | D1 | Complete test suite passes in the overlay with retry-once flake tolerance |
| 11 | SAST structural coherence | D1 | Overlay SAST has nodes and no orphan node-children edges |
| 12 | DSL no silent regressions | D1 | (MVP stub) Deferred to D3 post-apply check |
| 13 | Gap closure | D1 | Triggering gap report is absent from overlay SAST (gap was actually closed) |
| 14 | Migration safety | C6 | Proposed migration SQL contains no DROP TABLE, DROP COLUMN, or other destructive statements |
| 15 | Cross-codebase regression | D1 | (MVP stub, substrate only) Deferred to D3 fixture corpus |
| 16 | Extractor coverage | C6 | Extractor runs against positive and negative fixtures; row counts match expectations |
| 17 | Substrate consistency | C6 | Capability schema, extractor, and migration are self-consistent |
| 18 | Principle registry uniqueness | C6 | Proposed capability name does not collide with an existing registry entry |

Oracles 1, 2, 3, 6, 9, 14, 16, 17, 18 fire during their respective C stages and are verified from the audit trail at D1. Oracles 4, 5, 7, 8, 10, 11, 12, 13, 15 are new checks that run during D1 bundle assembly.

## Two bundle types

A **fix bundle** (`bundleType: "fix"`) covers the standard path: a bug has a known class, an existing DSL principle can express it, and the fix is a code change plus test plus principle reference. Oracles 1-13 apply.

A **substrate bundle** (`bundleType: "substrate"`) covers the extension path: the bug class cannot be expressed with existing DSL capabilities, so C6 proposes a new capability. Oracles 1-18 all apply. The bundle contains everything in a fix bundle plus a `CapabilitySpec` with a schema TypeScript file, migration SQL, extractor TypeScript, extractor tests, and a registry registration call.

Both bundle types are assembled atomically. A substrate bundle's migration and code commit are applied in the same transaction; if either fails, both roll back.

## Substrate self-extension

The key architectural move is that ProveKit can fix its own analysis gaps.

When C6's `tryExistingCapabilities` call returns `needs_capability`, the pipeline routes to `proposeWithCapability`. The LLM designs a new SAST capability: a SQLite table schema, a migration script, an AST extractor, fixture tests, and a DSL principle that uses the new capability. Oracle 14 rejects migrations that contain destructive statements. Oracle 16 executes the extractor against positive and negative source fixtures using `typescript.transpileModule` and a scratch database, then checks that row counts match the declared expectations. Oracle 17 checks that the schema, extractor, and migration refer to the same table names. Oracle 18 checks for name collisions.

If all four pass, the capability spec joins the bundle. When D2 applies the substrate bundle, the migration runs first, the extractor is registered, and the new capability is available for all future analyses. The next time an empty-catch bug arrives, the `try_catch_block` capability is already in the registry and C6 can express the invariant without proposing a new extension.

The compound effect: every substrate bundle applied makes the system strictly more capable. The gap list in `docs/plans/2026-04-23-fix-loop/capability-gaps.md` is the road map.

## Capture-the-change

The LLM does not produce JSON patches. In agent mode, it edits files directly inside the overlay worktree using the same tools a human would: Read, Edit, Write, Bash. After the agent call returns, `captureChange.ts` runs `git diff --name-only` and `git ls-files --others --exclude-standard` to find every file the agent touched, reads each file's current content, and assembles a `CodePatch`. The oracles verify the result, not the prompt. If `git diff` shows the wrong change, the oracle fails.

The JSON-patch path is a fallback for providers that do not implement `agent()`. `StubLLMProvider` in tests can provide either canned JSON responses or canned agent responses (pre-specified file edits) depending on what the test needs.

The `agent()` call goes through the CLI bridge (`cli.fix.ts`). Early in development the bridge only forwarded `complete()`, so every C-stage's `if (llm.agent) { ... }` dispatch silently fell back to the JSON path even though the Claude Agent SDK's `agent()` was wired at the provider level. The bridge now conditionally forwards `agent()` when the underlying provider implements it.

## Overlay isolation

The overlay is a scratch git worktree created from HEAD. Its purpose is to contain all code changes during generation so the main repository is never touched during a fix run.

The worktree boundary must be enforced at the tool level, not just by instruction. Early real-LLM runs showed that C-stage prompts that contained absolute paths (e.g. `/Users/tsavo/dogfood-scratch/src/divide.ts`) allowed Claude to edit files in the user's actual source directory via Edit and Write, bypassing the overlay completely. The fix was architectural, not a prompt tweak.

Overlay isolation now has two layers:

1. **Sanitized prompts.** Every C-stage prompt uses overlay-relative paths only. Absolute paths to source files are never included in LLM-visible context.

2. **Post-validation.** After each tool call that touches the filesystem, the path is checked against `overlay.worktreePath`. A path that escapes throws `OverlayBypassError` immediately, before the tool result is processed. This is the hard stop; the sanitized prompt is defense in depth.

`OverlayBypassError` is not a retryable failure. It terminates the current stage and surfaces as a pipeline error so the operator can inspect the tool calls that escaped. The dual-layer design follows the principle that trust boundaries must be enforced at the boundary, not by assuming inputs will be well-formed.

## Logging architecture

ProveKit uses Pino with dual output streams.

**The log file** (`.provekit/fix-loop-<ts>.log`) captures the full NDJSON transcript of every fix run: every LLM prompt, every LLM response, every thinking block the SDK exposes, every tool call with full parameters, every tool result, and every oracle verdict. Truncation in log files is forbidden. The convention and rationale are documented in `docs/LOGGING.md`.

**Stdout** receives a pretty-printed summary: stage markers, timing, pass/fail verdicts, and error context. It is a UX view of the same events, not the record of them.

The separation is load-bearing. During real-LLM dogfood, an overlay-bypass bug (Claude writing to absolute paths outside the worktree) was invisible for an afternoon because the SDK's default summary elided tool inputs. Once tool inputs were captured in full in the log file, the bypass was immediately visible in post-hoc replay.

Disk pressure is managed by capping the count of retained log files, not by truncating individual entries. Sensitive fields (API keys, secrets) are redacted at the named field level, not by lopping surrounding content.

## What is in code vs data

**Code (the pipeline itself):** the orchestrator that wires C1 through D3; the four registries and the artifact-kind registry; the 18 oracles' evaluation logic; `captureChange.ts`; the SAST builder and extractor infrastructure; the Z3 binding and verifier.

**Data (what lives inside the registries):** capability table definitions, migration SQL, extractors, DSL relations, remediation layer descriptors, intake adapter functions, artifact kind descriptors, principles, teaching examples. Every item in each registry is data that can be added by landing a substrate bundle. The pipeline composition does not change when a new capability lands. Only the registry grows.

This separation is the property that makes ProveKit self-extending. The oracles evaluate data. The pipeline is the fixed infrastructure that those oracles run against.
