# Standing Invariant Runtime (v1)

**Date:** 2026-04-27
**Status:** Spec, ready for implementation plan
**Scope:** Convert the fix loop's per-bug proof bundle into a continuously
  re-checkable spec the codebase pledges to satisfy. Compose with the
  existing fix loop and shadow AST substrate.

## What this is

The fix loop produces a Z3-grounded, mutation-verified, source-bound
invariant for every bug it ships. Today that invariant lives in the run's
audit trail and dies the moment the run completes. v1 of the standing
runtime turns each invariant into a permanent, source-controlled
obligation that the codebase pledges to satisfy on every commit, checked
mechanically, no LLM in the verification path.

The architectural claim: every fix shipped through provekit *permanently
shrinks the bug surface forward*. Existing bug surface shrinks (regression
test). Future regression surface shrinks (Z3-checkable invariant fires on
recurrence). Architectural-drift surface shrinks (binding decay alarm
fires before silent regression). Bug surface compounds DOWN.

This is impossible with grep + tests. Tests are point-in-time. Grep is
content-addressable. Only a source-bound, machine-checkable spec layer
gives you a permanent obligation that survives the codebase evolving
around it.

## Motivation

The fix loop has a known leak. Inspecting the dogfood proof shipped
2026-04-27 against the planted asc/desc bug:

The shipped bundle catches **one** failure mode out of four:

1. **The patched function itself reverts.** Regression test fires. ✓
2. **Upstream data-path refactor.** Someone rewrites `evolve` to fetch via
   a different repository method. The protected sink's data shape changes;
   the regression test still passes (it exercises forRevision, not the new
   path). The bug returns through the new path silently. ✗
3. **Schema/precondition drift.** New column changes the meaning of `date`.
   Patch satisfies the old invariant; correctness now requires a stricter
   one. ✗
4. **Bug-class spread.** New repository method gets added with the same
   asc/desc oversight. Original invariant scoped to forRevision doesn't
   fire on the new method. ✗

The shipped bundle is a snapshot proof, not a standing obligation.

Cases 2-4 reduce to a static-analysis problem with three known inputs:

1. The invariant (Z3 assertion + bindings)
2. The callsite (AST node the patch landed on)
3. All AST paths to that callsite (reverse dataflow + caller chain)

Whole-program reachability + dataflow + Z3, no LLM in the verifier. The
shadow AST gives the path enumeration. The invariant gives the property
to check at each path's tail. Z3 gives the decision procedure.

## Intake unification (v1)

The fix loop's intake supports two equally-valid input shapes, both
processed by the same downstream pipeline:

- **Prospective intake.** A user-filed problem statement. The change
  hasn't happened yet. Same shape as the dogfood proof on 2026-04-27.
- **Retrospective intake.** An existing commit (or a proposed commit
  about to land). The intent extractor reads diff + commit message,
  derives intent from what's there, mints constraint candidates and
  identifies missing regression tests. Same downstream gates apply
  (Z3 SAT, fidelity check, mutation verification, no-existing-violation).

Both intake directions converge on a single canonical artifact: the
**intent report**, a structured JSON document the rest of the pipeline
consumes. The intent report is also the bundle's primary output —
diffable, source-controlled, queryable. Schema:

```json
{
  "source": "prospective" | "retrospective",
  "trigger": {
    "kind": "problem_statement" | "commit",
    "ref": "user-text or commit-sha",
    "diff": "<unified diff if retrospective>",
    "commitMessage": "<message if retrospective>"
  },
  "intents": [
    {
      "lineRange": [42, 48],
      "filePath": "src/store/sqlite/repositories.ts",
      "intent": "ensure most-recent K invocations reach evolve",
      "hasRegressionTest": false,
      "testGenerationOpportunity": true,
      "constraintCandidate": {
        "smtSketch": "(assert (and ...))",
        "kind": "order",
        "validationStatus": "candidate" | "z3_sat" | "passed_oracles" | "rejected"
      }
    }
  ],
  "outputBundle": {
    "patch": "<diff if change is needed>",
    "addedTests": ["<test code if missing>"],
    "constraintArtifact": ".provekit/invariants/<sha>.json"
  }
}
```

Stage B0 (new) sits before Investigate in the pipeline and produces the
intent report. Its inputs are either a problem statement (prospective)
or diff + commit message (retrospective). Its outputs are zero or more
intents, each with constraint candidates and missing-test flags.

The retrospective direction enables two operations the prospective-only
loop cannot:

1. **Bootstrap from history.** `provekit mine-history` runs B0 in batch
   over every commit in the existing log, populating the constraint
   corpus from changes nobody filed problem statements for. A
   five-year-old codebase arrives at adoption with thousands of mined
   intents on day one.
2. **Self-test missing-test gaps.** When an intent ships without a
   corresponding regression test, the fix loop's output bundle includes
   the missing test. Vibe-coded codebases that ship features without
   tests get tests written by the substrate as it mines intent. Test
   coverage grows mechanically alongside the constraint corpus.

The intent extractor itself is an LLM call (same spiky-intelligence
properties as C1) and falls under the same gating posture: extracted
intents that don't yield Z3-SAT-able constraints don't ship; intents
whose constraint candidates fail oracle 1.5 fidelity don't ship; intents
whose generated tests fail mutation verification don't ship. The intent
extractor proposes; the gates dispose. No LLM in the verification path.

## Non-goals (v1)

- **Symbolic node identity across renames/extractions/moves.** Defer to v2.
  v1 uses content-addressable bindings (sha256 of node content); a rename
  decays the binding and surfaces as a yellow alarm asking the human to
  re-run the fix loop on the renamed locus. Cosmetic-edit decay is a
  tractable tax. Symbolic identity is an open research problem and gating
  v1 on solving it is the perfect-as-enemy-of-shipping mistake.

- **Cross-codebase invariant porting.** That's the principle library (C6),
  a separate surface. The standing runtime is per-codebase enforcement;
  the principle library is cross-codebase teaching.

- **LLM in the verification path.** Banned by design. The LLM's job is
  bounded to "what is the SMT property this code must satisfy?" — that's
  C1. Everything afterward is mechanical.

## Goals

- Source-controlled, content-addressable invariant store at
  `.provekit/invariants/<sha>.json`.
- Path enumerator over the shadow AST: `pathsTo(callsiteNodeId): Path[]`.
- Path-level Z3 checker: per-path symbolic execution + invariant evaluation.
- `provekit verify` CLI with three verdict categories: holds, decay, violation.
- Cross-path adversarial scan as a flag on verify.
- Cache layer keyed on binding hashes; typical commit re-checks <5% of
  invariants in <5 seconds.

## Architecture

Five composable pieces, each independently testable and shippable:

```
fix loop produces invariant
    │
    ▼
.provekit/invariants/<sha>.json  ◄── invariant store
    │
    ▼
provekit verify
    │
    ├── re-resolve bindings ──► binding-resolver
    │       │
    │       ▼
    │   nodeId still resolves? ──── no ──► DECAY verdict
    │       │
    │       yes
    │       ▼
    ├── enumerate paths ──────► path-enumerator (shadow AST + dataflow)
    │       │
    │       ▼
    └── check each path ──────► z3-path-checker
            │
            ▼
       per-path verdict: holds / violated / undecidable
            │
            ▼
       aggregate report (exit code)
```

## Components

### 1. Invariant store

**Location:** `.provekit/invariants/<sha>.json`

**Identity:** Filename is the sha256 prefix of `(SMT assertion + bindings)`.
Two runs producing the same invariant write the same file. Idempotent.
Source-controlled. Diffable. The patch commit and the invariant file
travel together.

**Schema:**

```json
{
  "id": "sha256-prefix-16char",
  "createdAt": "2026-04-27T19:30:00.000Z",
  "originatingBug": "free-text user signal that motivated this invariant",
  "smt": {
    "kind": "arithmetic|set_uniqueness|cardinality|order|taint|other",
    "declarations": ["(declare-const ...)"],
    "assertion": "(assert (...))"
  },
  "bindings": [
    {
      "smt_constant": "out_of_order_pair_exists",
      "source_expr": "schema.invocations.date",
      "sort": "Bool",
      "node": {
        "filePath": "src/store/sqlite/repositories.ts",
        "nodeId": "sha256-prefix",
        "startLine": 120,
        "endLine": 120
      }
    }
  ],
  "callsite": {
    "filePath": "src/store/sqlite/repositories.ts",
    "nodeId": "sha256-prefix",
    "function": "InvocationRepository.forRevision",
    "startLine": 115,
    "endLine": 124
  },
  "scope": "callsite",
  "regressionTest": {
    "filePath": "src/repositories.regression.test.ts",
    "testName": "regression: forRevision returns most-recent K"
  },
  "patchSha": "df9a9e36ef6045ea558073078416921d4ae3f317",
  "retired": null
}
```

**`scope` field:**
- `"callsite"` (default): the bound nodes ARE the function under test. Path
  enumeration walks backward from the callsite.
- `"sink"`: the bound nodes are the data destination (e.g., the data
  consumed by an LLM call, the data shown to a user). Path enumeration
  walks backward from the sink across the entire dataflow graph.
  `--adversarial` mode targets sink-scoped invariants.

**Retirement:** explicit. `provekit invariant retire <id> --reason "<text>"`
writes a tombstone to the `retired` field with timestamp + reason. Retired
invariants are skipped by `verify` but kept in the store for audit.

### 2. Binding resolver

**API:** `resolveBindings(invariant: Invariant, currentSubstrate: Substrate): ResolveResult`

Given an invariant and the current shadow AST + node hashes, attempt to
re-resolve every binding's `nodeId` against the current substrate.

**Outcomes per binding:**
- `resolved`: nodeId still hashes to the same content in the same file
- `decayed_moved`: a node with the same content hash exists, but in a
  different file/parent (file was moved, function was extracted)
- `decayed_changed`: file exists, but no node with the bound nodeId
  (content was edited)
- `decayed_deleted`: file no longer exists

`resolved` paths proceed to verification. Any `decayed_*` outcome surfaces
the invariant in the decay bucket.

**v1 simplification:** treat all `decayed_*` outcomes the same — alarm,
human re-runs fix loop. v2 can distinguish (e.g., `decayed_moved` could
auto-refresh the binding without LLM involvement).

### 3. Path enumerator

**API:** `pathsTo(callsiteNodeId: NodeId, substrate: Substrate): Path[]`

**Inputs:**
- callsite node id
- shadow AST + dataflow edges (already in substrate, written by SAST builder)

**Output:** array of paths, where each path is an ordered list of
`{nodeId, role}` from a data source to the callsite.

**Algorithm:** reverse BFS over dataflow edges, deduplicating by node-set,
capping at K paths (K=50 default, configurable per invariant).

**Termination:**
- Cycle: deduped node-set, cycle ignored
- Source: a node with no incoming dataflow edges (literal, parameter, repository read, external input)
- Cap: K paths returned, more enumerable on demand via `--max-paths`

**Path roles:** each step in a path is tagged with its semantic role
(`assignment`, `argument`, `return`, `db_read`, `parameter`, `literal`,
etc.). The Z3 checker uses roles to decide how to propagate constraints.

### 4. Z3 path checker

**API:** `checkPath(path: Path, invariant: Invariant): PathVerdict`

**Verdict:** `holds | violated | undecidable`

**Algorithm:**
1. Symbolic execution forward from the path's source, propagating
   constraints through each node's transformation. Each node emits Z3
   constraints based on its role.
2. At the callsite, evaluate the invariant's bindings against the symbolic
   state. The `source_expr` of each binding is mapped to the symbolic
   variable in scope at that point.
3. Z3 query: `source constraints AND path constraints AND NOT invariant`.
   - SAT: path can violate the invariant. Verdict: `violated`. Z3 model
     attached as the witness.
   - UNSAT: path cannot violate. Verdict: `holds`.
   - timeout: Verdict: `undecidable`. Soft warning; not a CI fail.

**Performance bound:** each path is a Z3 query, capped at 30s default.
Total budget is `K * timeout` per invariant in the worst case. Cache
short-circuits this for unchanged paths.

**v1 limitation:** symbolic execution is best-effort. Loops, recursion,
and external calls model as nondeterministic havoc; Z3 will return
`undecidable` more often than a research-grade analyzer. That's fine —
undecidable is honest, not a regression.

### 5. `provekit verify` CLI

**Command:** `provekit verify [--ci] [--invariant <id>] [--adversarial] [--max-paths N] [--timeout SECONDS]`

**Behavior:**
1. Load every invariant in `.provekit/invariants/` (skipping retired ones)
2. For each: resolve bindings against current substrate
3. Resolved bindings → enumerate paths → check each path
4. Decayed bindings → emit decay verdict, skip path checks
5. Aggregate report

**Output (`--ci` mode):**

```
provekit verify: 12 invariants
  ✓ holds (10): forRevision-most-recent-k, divide-guard, ...
  ⚠ decay (1): order-by-date-asc
      file: src/store/sqlite/repositories.ts:120
      reason: bound nodeId 4f7c... no longer resolves; content changed
      remediation: re-run `provekit fix` on this locus or retire the invariant
  ✗ violated (1): unique-keys-in-allow-header
      file: src/lib/http.ts:42
      via path: <node-list>
      Z3 model: <witness>
      remediation: fix the highlighted path

cache: 8/12 hit, 4 re-evaluated (3.2s)
```

**Exit codes:**
- 0: all invariants hold or undecidable
- 1: at least one violation
- 2: at least one decay (no violations)
- 3: internal error (Z3 crashed, substrate unreadable, etc.)

**CI integration:** git pre-commit hook can run `provekit verify --ci`.
Cache lookup keyed on binding hashes means typical commits re-check only
invariants whose bindings touch changed files. Target: <5s wall time on
a 100-invariant repo with a typical 5-file commit.

### 6. Adversarial cross-path scan

**Trigger:** `provekit verify --adversarial`

**Targets:** invariants with `scope: "sink"`.

**Algorithm:** for each sink-scoped invariant, enumerate ALL paths in the
codebase that could feed the sink (not just paths to the original
callsite). Check each path with the same Z3 checker.

**Catches:** "new method introduced with same bug class." Example:
the asc/desc invariant from the dogfood proof, if scoped to the sink (the
data consumed by the evolve meta-prompt) instead of the callsite
(forRevision), would fire on any new repository method that feeds evolve
without descending order. Cases 2 and 4 from the motivation become
detectable.

**Cost:** O(graph size) instead of O(paths-to-one-callsite). Default off;
opt-in per CI run or per invariant via `--invariant <id> --adversarial`.

### 7. Cache layer

**Cache key:** sha256 of `(invariant.id, every binding's resolved nodeId, every binding's resolved content hash)`.

**Cache value:** previous `PathVerdict[]` for that invariant.

**Invalidation:** any binding's resolved nodeId or content hash changes.

**Storage:** `.provekit/cache/verify.json`. Can be deleted; rebuilds on
next run. Not source-controlled (gitignored).

**Effect:** on a clean repo (no changes since last verify), 100% cache
hit, <100ms total. On a typical 5-file commit, ~5% of invariants
re-evaluated, <5s total. The cache is what makes pre-commit-hook
integration tractable.

## Decay semantics

A binding decays when the substrate no longer agrees with what the
invariant claims it bound to. v1 surfaces decays as exit-code-2 with a
yellow alarm. The remediation is one of:

1. **Cosmetic edit (rename, reformat):** re-run `provekit fix` on the
   renamed locus. The fix loop re-derives the invariant against the new
   substrate; the new invariant gets a new sha256, the old one is retired
   with reason `"superseded by <new-id>"`.

2. **Architectural change (data path removed/replaced):** invariant no
   longer applies. Run `provekit invariant retire <id> --reason "..."`.

3. **Drift signal (the bound concept no longer exists in any form):** the
   decay IS the signal. The codebase has moved out from under a
   correctness pledge. Human investigates whether the pledge is still
   wanted; if yes, re-run fix loop on the new locus; if no, retire.

The decay is a feature, not a bug. It surfaces architectural movement
through protected concepts, which is exactly the moment a human-in-the-
loop confirmation is wanted.

## Performance budget

| Scenario | Target |
|----------|--------|
| Clean repo (no changes since last verify) | <100ms (100% cache hit) |
| Typical commit (5-10 files) | <5s (5% re-eval) |
| `--adversarial` on a 100-invariant repo | 30s-2min |
| Worst case (every binding decayed, full re-eval) | <60s for 100 invariants |

These are targets, not contracts. The cache is the load-bearing piece. If
cache hits drop below 90% on typical commits, the runtime is too slow
for a pre-commit hook and the design has failed.

## Implementation order

Each step is independently shippable; users get value at each stage.

1. **Invariant store** (schema + emitter from fix loop + reader from CLI).
   Smallest piece. Blocks everything else. Ship first.
2. **`provekit verify` skeleton:** load store, re-resolve bindings, report
   decays only. No path enumeration, no Z3. This alone is useful: tells
   users which of their invariants are stale.
3. **Path enumerator.** Self-contained, testable in isolation against the
   substrate.
4. **Z3 path checker.** Composes with #3. Testable against synthesized
   path/invariant pairs.
5. **Wire #3 + #4 into `provekit verify`.** Now `verify` returns full
   verdicts.
6. **Cache layer.** Performance pass; cache invariant verdicts keyed on
   binding hashes.
7. **Adversarial scan flag.** Composes; minimal new code.
8. **B0 prospective intake** as the v0 entry path (already exists in the
   shipped fix loop, formalize the intent-report output shape).
9. **B0 retrospective intake.** Read diff + commit message, run intent
   extractor, produce intent report. Reuses every downstream gate that
   B0 prospective uses. No new mechanical infrastructure beyond the
   intent extractor itself.
10. **Missing-test generation.** When B0 (either direction) reports an
    intent without a regression test, plumb that signal into C5 so the
    output bundle includes the missing test as a first-class artifact.
11. **`provekit mine-history`** CLI: runs B0 retrospective in batch over
    the existing commit log. Bootstrap-from-history product surface.

Steps 1-2 give a working `verify` that detects decays. Steps 3-5 add
violation detection. Step 6 makes it CI-fast. Step 7 catches bug-class
spread.

## Acceptance criteria

The runtime is shippable when:

1. The fix loop's existing dogfood proof (planted asc/desc bug in promptlib)
   continues to ship a bundle, AND emits an invariant to
   `.provekit/invariants/<sha>.json`.
2. `provekit verify` on the post-fix promptlib repo: green (1 invariant,
   holds).
3. Manual revert of the fix in promptlib: `provekit verify` reports
   1 violation with the correct path and Z3 witness.
4. Manual rename of `forRevision` to `forRevisionDesc` in promptlib:
   `provekit verify` reports 1 decay with the correct remediation.
5. Adding a new repository method `forRevisionUnsorted` that hits the same
   sink: `provekit verify --adversarial` reports 1 violation against the
   new method.
6. Performance: a 100-invariant test repo verifies in <5s on a typical
   commit, <100ms with no changes.

## Part B as a plugin: the constraint-minting interface

The runtime is one half of a two-part architecture (the other half is
the constraint-minting pipeline; see the constraint-driven-development
doc for the full A/B framing). This spec is about Part A — the gate
that consumes invariants. Part B is the producer side, and B is
**explicitly a plugin slot, not a fixed implementation**.

ProvekIt ships a reference B that uses claude-agent-sdk + ts-morph +
git worktrees. Integrators (IDEs, agent runtimes, on-prem deployments)
can swap the entire B with their own implementation without touching A.

### The B-plugin contract

Any valid Part B implementation must produce two artifact shapes:

1. **An `IntentReport` JSON object** matching the schema in the "Intake
   unification (v1)" section above. This carries the derived intent
   plus any constraint candidates plus per-intent flags
   (hasRegressionTest, testGenerationOpportunity).

2. **A `StoredInvariant` JSON file** at
   `.provekit/invariants/<id>.json` for any constraint that survives
   the downstream gates (Z3 SAT, fidelity checks, mutation
   verification). Schema is the one defined in this spec's invariant
   store section.

That's the contract. Anything that produces those two artifacts is a
valid B. The reference implementation uses our LLMProvider abstraction,
runAgentInOverlay (claude-agent-sdk-based), our overlay/git-worktree
sandbox, and our diff-and-patch flow. None of those are part of the
contract; they're implementation choices.

### What integrators can swap

When a third-party B replaces ours, these pieces change:

- **LLM.** Already swappable via the LLMProvider abstraction. An
  integrator's B may not use LLMProvider at all — they might call
  their LLM through a totally different layer.
- **Language-server bindings.** Our reference uses ts-morph to walk the
  AST. An IDE that has an LSP already knows the codebase; their B can
  bind there instead of re-parsing.
- **Toolpath.** How tools (Read, Edit, Write, Bash, etc.) are invoked
  on behalf of the LLM. The claude-agent-sdk's tool model is one shape;
  Cursor's is another; an enterprise sandbox has its own. Our B uses
  the SDK; theirs uses whatever they already operate.
- **Code sandbox.** Where generated code runs during validation. We use
  git worktrees on local disk; alternative B implementations can use
  in-process sandboxes, containers, or remote microVMs.
- **Diff process.** How proposed code changes are represented and
  applied. We produce unified diffs against a worktree; an IDE's B
  might produce in-editor edits via the IDE's diff model.
- **PR flow.** Where the output ends up. We emit `provekit-fix.patch` +
  `provekit-fix.md`. An integrator's B might open a PR through the
  IDE's source-control API, post to a ticket system, or drop a
  follow-up commit.

### What stays invariant across B implementations

Part A is invariant. The runtime gate consumes the artifact shapes B
produces and processes them identically regardless of how B got there.
This is what makes "BYO B" tractable: integrators don't have to
re-implement the gate; they just have to produce its inputs.

The contract surface is therefore the smallest possible Part-A-Part-B
coupling: two JSON shapes (`IntentReport`, `StoredInvariant`) and one
filesystem location (`.provekit/invariants/`). Everything else inside B
is the integrator's choice.

### Strategic consequence

Part A is the moat — small, mechanical, free, ubiquitous. Part B is the
moving target — improves with every frontier-model release on someone
else's R&D budget. ProvekIt's reference B exists to bootstrap adoption;
it's not the long-term product. The long-term product is the gate plus
the constraint corpus it accumulates per codebase. B is a plugin that
gets better around it.

## Distribution surface

The runtime ships through two artifacts; everything else is downstream
of these.

**Artifact 1 — `provekit` CLI binary + GitHub Action.** Single Node
package, single entry point, single command. The CLI exposes:
`provekit verify` (the standing-runtime gate), `provekit fix` (the LLM
pipeline), `provekit invariants list/verify/retire/paths` (the
constraint-store inspection commands), `provekit mine-history` (the
historical-bootstrap command). The GitHub Action wraps `provekit verify`
and exposes its verdict to the existing PR check surface every developer
already understands. Channel 1: every developer adds it to their CI.

**Artifact 2 — Library entry points.** TypeScript imports any IDE,
agent runtime, or platform can integrate. The four canonical entry
points: `runFixLoop` (full pipeline), `verifyAll` (the standing-runtime
gate), `extractIntent` (B0 retrospective), `readInvariants` /
`writeInvariant` (constraint store I/O). Channel 2: every IDE
integrates ProvekIt to prove correctness; Holyship integrates it as a
gate in its gate library; future agent runtimes plug into the same
surface.

**What's not in the distribution surface.** Linear webhooks, Slack
bots, GitHub Issues subscribers, email connectors, per-IDE plugins,
custom event-bus adapters — these are third-party integrations
downstream of the CLI and library. ProvekIt doesn't ship or maintain
them. Integrators write them by composing the two artifacts above.

The acceptance criteria below verify both artifacts: criterion #6 (the
performance budget) is the GitHub Action / CLI check; the existence of
clean library entry points is verified by the fact that the CLI itself
imports them.

## What this unlocks

The marketing claim becomes provable:

*ProvekIt doesn't fix your bugs. ProvekIt makes the bug class permanently
un-shippable. Every fix is a contract your codebase pledges to keep.
Refactor freely; the contracts move with you.*

The fix loop is bait — immediately useful, ships day one. The standing
runtime is the moat — impossible to replicate with grep + tests because
correctness compounds in only one direction with point-in-time tools, and
this construction reverses that.

Composes with the existing principle library: principles teach across
codebases, invariants enforce within them. Two surfaces, both load-bearing,
neither replaceable by the other.

## Open questions for v2

- Symbolic node identity across renames/extractions/moves (deferred from v1)
- Distributed invariant stores (one repo's invariants imported into another)
- Invariant "shapes" — parametric invariants that adapt to a target
  codebase (the bridge between the principle library and the per-codebase
  invariant store)
- Witness-replay: when a violation is found, mechanically synthesize a
  failing test case from the Z3 witness
- Time-decay metadata: invariants tagged with their motivating bug class,
  so a violation report can name "this is the same bug class as X from
  six months ago"

These are v2+. v1 ships without them and remains useful.
