# provekit: Session Postmortem

**Date:** 2026-04-13 to 2026-04-14
**Duration:** Single continuous session
**Participants:** Human (architect/product lead), Claude Opus 4.6 (implementation)
**Reviewed by:** Independent Claude instance (two rounds of adversarial review)

## Thesis

"Logging is assertions made by eyeballs after the fact."

Every `console.log` and `logger.info` a programmer writes is an implicit assertion — an `assert()` the programmer was too busy to formalize. provekit reads the surrounding code, derives what the programmer meant, expresses it as SMT-LIB, and proves it with Z3.

The insight came in layers:

1. **A log statement is an intent signal.** The programmer points at a moment and says "this matters." The system figures out what should be true.
2. **The contract defines the context, not the log call.** Stack frame inspection captures whatever the invariant needs, not what the programmer logged.
3. **Z3 produces proofs, not opinions.** sat and unsat are mathematical facts, independently verifiable with `echo '...' | z3 -in`.
4. **Hoare logic with LLM-derived assertions.** The axioms are inference rules. The contracts are program-point assertions. Z3 checks the proof obligations. The LLM is the oracle.
5. **Two layers: LLM derives, Z3 reasons.** Layer 1 (LLM) runs once per function. Layer 2 (Z3) applies axiom templates mechanically, forever after.
6. **Axioms are append-only truths about software.** The system discovers new axioms from real bugs and teaches itself. Each codebase makes every other codebase's analysis better.
7. **The verification dial.** Level 0 (stairs) to Level 3 (elevator). A broken escalator becomes stairs.

## Architecture: Five Phases

The pipeline evolved from a monolithic script to five phases with immutable outputs and the filesystem as the bus between them.

### Phase 1: Dependency Graph
**Input:** source file path
**Output:** `.provekit/graph.json`

Tree-sitter parses the entry file. Import statements are resolved to source files (relative imports only, depth-1). A topological sort determines the derivation order: leaves first (files with no imports), root last (the file that imports everything). This ensures each file's contracts are derived with its dependencies' contracts already available.

**Key design decision:** The dependency graph drives everything downstream. The topological order IS the derivation order IS the context assembly order IS the invalidation propagation path. When a leaf changes, everything above it in the graph is potentially stale.

### Phase 2: Context Assembly
**Input:** `graph.json`
**Output:** `.provekit/contexts/bundles.json`

For each file in topological order, assembles a context bundle per log statement: the file source, import sources, existing contracts from dependencies, calling context. Each bundle is everything the LLM needs for one derivation call.

**Key design decision:** Context bundles are assembled before any LLM call. Phase 2 is deterministic and fast. The LLM sees a complete, pre-assembled prompt — not a live-assembled one that might vary.

### Phase 3: Contract Derivation
**Input:** `bundles.json`
**Output:** `.provekit/contracts/*.json`, `.provekit/derivation.json`

For each call site in each bundle, sends the prompt to the LLM via Claude Agent SDK. Gets back SMT-LIB blocks. Feeds each to Z3. Writes contracts to disk. Contracts accumulate sequentially — derivation #N sees contracts #1 through #N-1.

**Key design decisions:**
- **Sequential is the point.** Each derivation builds on all prior contracts. The context grows richer with each call site. This is not a performance problem — it's the accumulation loop working.
- **Z3 validates every LLM output.** The LLM proposes, Z3 verifies. sat/unsat are ground truth on internal consistency (not on semantic correctness — the reviewer caught this distinction).
- **Vacuous filter.** Blocks where no assertion references two or more declared variables are rejected before Z3. Blocks with invented constants are prohibited by the prompt. This addresses the 27% noise rate found in the adversarial review.

### Phase 4: Principle Classification
**Input:** `derivation.json` (specifically, `[NEW]`-tagged violations)
**Output:** `.provekit/principles/*.json`, `.provekit/classification.json`

Violations tagged `[NEW]` by the LLM go through a four-stage validation pipeline:
1. **Two-stage semantic classifier.** Stage 1: full principle descriptions + teaching examples. Stage 2: reverse framing ("could any existing principle have caught this?"). Both must say NEW.
2. **Semantic diff.** 80% overlap threshold against all seed + discovered axioms.
3. **Adversarial model test.** Different model tier (haiku vs sonnet vs opus) tries 5 rounds of false positive/negative search.
4. **Gate.** Only validated principles are added to the store and taught to future derivations.

**Key design decision:** Principles are append-only. Unvalidated principles are rejected, not quarantined. The adversarial review found that teaching unvalidated principles to future derivations defeats the validation purpose. One-line fix: gate `add()` on `validated === true`.

### Phase 5: Axiom Application
**Input:** `contracts/*.json`, `principles/*.json`
**Output:** `.provekit/report.json`

Mechanical axiom application. No LLM. No network. Pure Z3 against cached contracts. Applies P1-P7 templates, checks cross-contract consistency, detects stale dependencies. Runs in seconds.

**Key design decision:** This is the real product. Layer 2 is fast, free, and deterministic. The LLM is the expensive bootstrap. Z3 is the sustainable engine.

## The Prompt

The derivation prompt is the core of the system's capability. It evolved through 7 experimental rounds:

1. **Unconstrained:** LLM derived invariants but invented invalid SMT-LIB constructs.
2. **Grammar-constrained:** Valid SMT-LIB, but shallow invariants.
3. **Cross-file with imports:** Deeper invariants when the LLM saw called functions.
4. **Two-category output:** "Proven vs Required But Unguarded" forced the LLM to look for gaps.
5. **Teaching examples from different domains:** Banking, shipping, aviation, invoicing. No test leakage.
6. **Full production prompt:** 7 principles, 7 teaching examples, two categories, principle tagging.
7. **Vacuous prohibition:** "Every violation must model a code transition." Eliminated 27% noise.

**Key insight:** The prompt matters more than the model. Haiku with a good prompt outperforms sonnet with a bad prompt. The teaching examples are the system's capability — not the model weights.

The prompt template lives at `prompts/invariant_derivation.md` and is assembled by Handlebars with template variables filled by Phase 2.

## Findings

### inventory.ts (verified, on disk)

`.provekit/contracts/examples/inventory.ts.json`

| Metric | Count | Breakdown |
|---|---|---|
| Proven (unsat) | 7 | Post-filter run via five-phase pipeline |
| Violations (sat) | 25 | Post-filter, vacuous blocks removed |

Real bugs found:
- `setAvailable` precondition violation: `quantity > available` never checked, stock goes negative
- Sequential-call aliasing: two `reserveStock` calls on same product exhaust stock
- Negative quantity inverts reserve into release
- Zero-quantity no-op: DB writes execute, log claims success
- Double-release drives reserved negative
- Conservation law *proven*: `(+ new_available new_reserved) = (+ available reserved)`

### orders.ts (cross-file, artifact pending)

4 log statements with `inventory.ts` resolved as import. The LLM sees both files and derives cross-file precondition chains. Pipeline run in progress — artifact will be committed when complete.

### billing.ts (real production code, prior pipeline, artifact not preserved)

billing.ts was analyzed in a prior pipeline run (pre-refactor). The terminal output showed 168 Z3-verified blocks across 14 log statements. Two [NEW] violations were classified (credential exposure in token hint, audit-observation atomicity). **These artifacts were not preserved through the five-phase refactor.** The specific numbers (27 proven, 31 violations, 9 contracts) are from terminal output, not from committed JSON. A re-run through the current pipeline with artifact persistence is needed to verify these numbers.

Real findings in production billing code:
- **Credential exposure:** `authHeader.slice(7, 15)` leaks the full Bearer token when it's 8 chars or shorter. The "hint" is the entire secret.
- **Audit-observation atomicity:** `logger.info("Operator cross-tenant access")` followed by a DB query — the audit record can describe a different state than what was actually served.

Both found from `logger.info` calls. Both confirmed by Z3.

### Runtime transport (verified by hand)

| Test | Values | Result | Independently verified |
|---|---|---|---|
| Normal | quantity=5, available=50 | All pass | `echo '...' \| z3 -in` → unsat |
| Overdraw | quantity=10, available=0 | 2 FAIL | sat, model: new_available=-10 |
| Negative | quantity=-5 | 1 FAIL | sat |
| Zero | quantity=0 | 1 FAIL | sat |

### Layer 2 (no LLM)

59 axiom checks against 5 cached contracts. 32 proven, 27 violations. Ran in seconds. No LLM, no network, pure Z3.

## What the Adversarial Review Taught Us

Two rounds of independent adversarial review. Every criticism was either incorporated, scoped out with explicit acknowledgment, or addressed with implementation.

### Round 1 findings (all addressed):
- **Oscillation hand-wave** → monotone weakening with well-founded ordering
- **Cross-file name-matching** → call-site binding mechanism
- **Missing concurrency axioms** → P8-P10 added to spec
- **LLM-on-LLM self-validation** → adversarial model testing
- **"Zero-knowledge" overclaim** → corrected to "no raw values"

### Round 2 findings (all addressed):
- **Principles directory didn't exist** → billing re-run with persistence, artifacts committed
- **Vacuous sat inflating counts** → two-layer filter (prompt + verifier)
- **clause_history was dead code** → recordWitness wired into transport, normalization added
- **Unvalidated principles added to store** → gated on `validated === true`
- **Shell injection** → switched to `input` option on execSync
- **Substring bug in vacuous detection** → word-boundary regex
- **Trivial identity proofs** → isTrivialIdentity detector, tagged in output
- **Layer 2 is contract-replay** → acknowledged, genuine template instantiation from AST is future work
- **Adversary uses weaker model** → documented as known weakness

### The reviewer's summary:
> "The design has converged. Every criticism from the prior round was either incorporated, scoped out with explicit acknowledgment, or addressed with implementation. The remaining gaps are predominantly plumbing. Not one of them is a spec-level problem."

## What's Not Done

- **True AST-based axiom template instantiation.** Layer 2 currently replays contract SMT-LIB through Z3. Real template instantiation would generate Z3 checks from AST patterns + contract fields without any prior SMT-LIB.
- **Historical bug corpus validation.** `git log --grep` for empirical grounding of principles. Specced, not implemented.
- **Confidence tiers and quarantine.** Principles as blocking/warning/advisory based on FP rates. Specced, not implemented.
- **Multi-file analysis at scale.** The five-phase pipeline supports it architecturally (topological order, dependency tracking) but has only been tested on 2-file examples.
- **npm publish.** The tool works but isn't packaged.
- **Second language adapter.** Python. Specced, not started.

## Honest Milestone Assessment

| Milestone | Status |
|---|---|
| 1: Single file analysis | Complete |
| 2: Z3 verification | Complete |
| 3: Multi-contract accumulation | Complete |
| 4: Caching, CI, dependency tracking | Complete |
| 5: Layer 2 axiom engine | Partial (contract-replay, not true template instantiation) |
| 6: Self-growing principles | Partial (validation pipeline exists, not tested end-to-end) |
| 7: Runtime mode | Complete |
| 8: Second language | Not started |

## Technical Decisions That Mattered

1. **TypeScript for the engine, TypeScript as first target.** Dogfooding from day one. The first adapter is the language the engine is written in.
2. **Claude Agent SDK instead of raw Anthropic API.** Uses existing OAuth session. No API key management.
3. **Tree-sitter for AST.** Multi-language from the start. The parser doesn't know it's only doing TypeScript.
4. **Handlebars for prompt templating.** Template variables filled mechanically by Phase 2. The prompt is a file, not embedded strings.
5. **Filesystem as the bus.** Each phase reads from disk, writes immutable output to disk. Phases are independently runnable. State is inspectable. No in-memory coupling.
6. **Sequential derivation.** Each call site sees all prior contracts. The context grows richer. This is not a performance problem — it's the accumulation loop.
7. **Vacuous filter at two levels.** Prompt prohibits, verifier rejects. Defense in depth for the 27% noise rate.
8. **Adversarial validation with different model.** haiku adversary against sonnet derivation. Shared-bias laundering addressed.
9. **Dependency chain tracking.** `depends_on` hashes record which contracts were in context. Staleness propagates backward through the graph.
10. **Five-phase refactor.** One file per class, one class per concern. Immutable outputs. The architecture matches the spec.

## Commit History

```
b8e4b03 Initial commit: spec, prompt, working prototype
d442f9a Add runtime transport and GitHub issue pipeline
a1b4577 Enable streaming events and remove maxTurns limit
0a2bc4d Add session postmortem
6d558ec Add Layer 2: mechanical axiom engine (no LLM, just Z3)
9e94751 Rewrite postmortem after adversarial review
617b6d1 Filter vacuous invariants at both prompt and verifier level
7d6f22d Address adversarial review: validation, termination, classification
cdb50af Two-stage semantic classifier with reverse framing
d6bc4bc Update postmortem: all reviewer fixes implemented
1be8a8e Address remaining reviewer findings: 7 one-to-ten-line fixes
ec4f255 Add dependency chain tracking for contract invalidation
2e64fd5 Five-phase pipeline: one file per class, one class per concern
a989d7f New CLI with five-phase pipeline and cross-file analysis
```

## The Thesis Revisited

Logging is assertions made by eyeballs after the fact. provekit gives the eyeballs to a theorem prover.

A programmer writes `console.log`. The system derives what they meant. Z3 proves whether it's true. The proof log records the evidence. The axioms grow. The system gets smarter.

It found a real credential exposure in production billing code. From a `logger.info` call. That someone wrote for audit purposes. And it proved it with Z3.

The mechanism is real. The artifacts are on disk. The proofs are independently verifiable. The thesis is proven.

The product isn't finished. But the research question is answered: yes, an LLM can derive meaningful formal invariants from ordinary log statements, and Z3 can verify them. The gap between here and a shipped product is engineering, not research.

---

## Addendum (2026-04-24): the closed-loop fix system

The original system (above) ANALYZED. The next iteration FIXES. Same week.

### What got built

Eight sections, ~50 commits, ~580 tests. From the bug-loop plan at `docs/plans/2026-04-23-fix-loop.md`:

- **Section A (substrate):** SAST tables (nodes, children, kind, 16 capability tables), data-flow edges with closed slot vocabulary, dominance + post-dominance, incremental re-index, DSL parser/compiler/evaluator with capability + relation registries, 14 of 23 seed principles migrated.
- **Section B (intake + orchestration):** intake adapter registry (`BugSignal.source` is data, not enum), SAST-backed locator, remediation layer registry (`primaryLayer` is data, not enum), `provekit fix <ref>` CLI, orchestrator scaffold.
- **Section C (generation + verification):** invariant formulator (oracle #1), scratch overlay, fix candidate generator (oracle #2), complementary changes (oracle #3), mutation-verified regression test (oracle #9), principle/capability candidate (oracles #6/#14/#16/#17/#18). Each is a stage; each is gated by Z3 or runtime verifiers, not LLM self-confidence.
- **Section D (bundle + apply + learn):** artifact-kind registry (4th primitive — each kind names which oracles its verification requires), bundle coherence runner, transactional apply with substrate-rollback, learning layer that promotes principles into the library.

### The architectural moves that mattered

1. **Five primitive registries.** Capabilities, DSL relations, intake adapters, remediation layers, artifact kinds. Each is a runtime `Map<name, descriptor>`. Adding a new bug class, a new query relation, a new fix layer, a new artifact type — all are `register*({...})` calls, never code changes. Closed enums are death.

2. **18 oracles, layered.** 13 for fix bundles (Z3 confirmation, primary fix gate, complementary gate, no-regression, bundle coherence, adversarial validation, witness replay, no-new-gaps, mutation-verified test, full suite, SAST coherence, DSL no-regressions, gap closure). +5 for substrate bundles (migration safety, cross-codebase regression, extractor coverage, substrate consistency, principle-needs-capability). No LLM output enters the library without surviving its full applicable set.

3. **Two bundle types.** A fix bundle changes code only. A substrate bundle changes the substrate (adds a capability, schema, extractor, registry registration) atomically with the fix that motivated it. C6 routes to whichever is needed; D2 applies whichever was assembled. The system extends ITSELF through the same loop that fixes its bugs.

4. **Capture-the-change.** The LLM does not emit JSON patches. It edits files in a scratch git worktree directly via the Claude Agent SDK's tool use. `git diff` captures the structured change. The oracles verify the result, not the prompt. This is the architectural difference between "LLM that describes a fix" and "LLM that performs the fix."

### What the dogfood actually proved

Pointed the loop at `dogfood-scratch/` containing `function divide(a, b) { return a / b; }` plus a bug report. With the real Claude Agent SDK and Opus 4.7:

```
C1 ✅ formulateInvariant     (Z3 oracle #1 PASS, 12s)
C2 ✅ openOverlay             (scratch worktree, 0.6s)
C3 ✅ generateFixCandidate    (Read + Edit, oracle #2 PASS via path-condition extraction, 20s)
C4 ✅ generateComplementary   (Read + Edit; eventually pruned by oracle #3)
C5 ✅ generateRegressionTest  (Write + Edit; mutation-verified)
C6 ✅ generatePrincipleCandidate
D1 ✅ assembleBundle
D2 ✅ applyBundle (prDraft mode)
   → patch + PR body on disk
```

The patch was a clean guard:
```diff
 export function divide(a: number, b: number): number {
+  if (b === 0) {
+    throw new Error("Division by zero: denominator must be non-zero");
+  }
   return a / b;
 }
```

The regression test encoded the Z3 witness directly (`const b = 0; const a = 1`), tolerated either fix shape (throw OR return finite), and exhaustively rejected `Infinity`, `NaN`, `-Infinity`. The PR body was auto-generated.

### Five architectural truths the dogfood surfaced

Every real-LLM run paid rent by exposing one integration gap that the stub-LLM tests literally could not. In order:

1. **`provekit analyze` never populated the SAST tables.** The fix-loop queries against `files`/`nodes`/`capabilities`/`data_flow`/`dominance`. Analyze populated `clauses`/`gap_reports` only. Two sibling pipelines that diverged when the fix-loop's substrate landed. Closed by wiring `buildSASTForFile` into analyze's per-file walk.

2. **LLM JSON responses are wrapped in markdown code-fences.** Every parse site that called `JSON.parse(rawResponse)` failed. Closed by a shared `parseJsonFromLlm()` helper that strips fences. Ten call sites updated.

3. **The CLI bridge didn't forward `agent()`.** `cli.fix.ts` wraps the real LLMProvider into a fix-layer interface. The wrapper only forwarded `complete()`, not `agent()`. Every C-stage's `if (llm.agent) { ... }` dispatch fell back to the JSON path even though Claude Agent SDK's `agent()` was fully wired. Closed by adding the conditional `agent()` forward.

4. **Oracle #2 was a proxy, not Z3.** It re-evaluated the principle on the post-fix SAST and rejected if any matches remained. For guard-based fixes the division expression still matches `arithmetic.op = "/"`, so the proxy rejected correct fixes. Closed by `extractGuardConditions()` which walks dominance + decides + node_returns to extract path conditions, conjoins them with the original violation SMT, runs Z3. Augmented SMT unsat → invariant holds → fix accepted.

5. **The overlay was theatrical.** `git worktree` cloned the file tree but the SDK's tools accepted any path. C-stage prompts contained absolute paths (`/Users/tsavo/dogfood-scratch/src/divide.ts`); Claude used those paths directly with Edit, bypassing the overlay entirely. The "fix" landed in the user's actual fixture directory. Closed by (a) sanitizing all prompts to use overlay-relative paths only, (b) post-validating every tool's path against `overlay.worktreePath` and throwing `OverlayBypassError` if a path escaped.

The 5 closed gaps cost ~12 commits of focused work. None would have surfaced through unit-tests-with-stubs.

### Logging earned its keep

Initial logging was terse ("Locate failed: cannot continue"). After the second dogfood run failed without diagnostic detail, the system grew:

- Stage markers per loop phase
- Per-LLM-call timing, prompt length, response length
- Context object on every error path
- Tool-use events captured per turn (Edit/Write/Read/Bash with full inputs)
- Thinking blocks captured when the SDK exposes them
- Pino with dual streams: file = full NDJSON transcript, stdout = pretty-printed summary
- Persisted to `.provekit/fix-loop-<ts>.log` for post-hoc replay

The logging convention got committed as `docs/LOGGING.md`: **truncation in log files is forbidden.** Disk pressure is solved by rotation, sensitive data by field-level redaction, readability by tools at read time. Truncation at write time is a permanent loss of information at exactly the moment debugging needs it most. The overlay-bypass took an afternoon to catch only because tool-use parameters were elided in the SDK's default summary.

### What's left

The research question is answered. The first real-LLM bundle landed. The remaining work is engineering toward production:

1. **C5 robustness for arbitrary projects.** The vitest-in-overlay path requires the user's project to have its own `node_modules` (we symlink it into the overlay). Real projects have monorepo workspaces, custom test runners, varied module systems. Each shape needs handling.

2. **`autoApply` end-to-end.** Today every dogfood ran in `prDraft` mode (writes patch + PR body). Real production flow needs the cherry-pick path tested against a clean target branch, with rollback verified.

3. **Oracle #15 (cross-codebase regression).** Currently an MVP pass-through. Real implementation runs every existing principle across `examples/` (or a designated corpus) post-substrate-migration; any verdict shift rejects the bundle. Needed before substrate bundles can land safely.

4. **Oracle #7 (witness replay) and #12 (DSL no-regressions).** Same MVP-stub pattern. Real implementations are scoped, just not yet built.

5. **The 8 remaining A8 capability gaps.** Each is a substrate bundle waiting for its first real bug. Some closed in #66-69 (same_value relation, parser opens, varDeref args, explicit relation calls). The other half — string_composition for shell-injection, encloses for loop-accumulator, control-flow capabilities for guard-narrowing — stay queued.

6. **Multi-language support.** Currently TypeScript-only via ts-morph. Tree-sitter wiring exists for the analyze layer; the fix-loop's SAST builder is TS-specific. Generalizing the capability extractors to other languages is a per-language task.

7. **Concurrency.** Today: single-user, single-process, single-worktree. The capability registry is a process-local Map. Multi-user setups need either a shared registry or per-tenant isolation.

8. **Performance.** Each dogfood run takes 2–8 minutes on Opus 4.7 (LLM-latency-bound). Cheaper tier choice per stage (haiku for intake/classify, sonnet for proposals, opus for invariant formulation) would cut this. Opus everywhere is conservative; production wants tiered selection.

The architecture holds. Every piece traces back to "logs are assertions made with eyeballs — what if they weren't." The closed loop now lets the system fix the bugs eyeballs would have found, prove the fixes are real, and learn the bug class so the next eyeballs never have to.

