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
6. **Cache layer.** Last performance pass.
7. **Adversarial scan flag.** Composes; minimal new code.

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

## What this unlocks

The marketing claim becomes provable:

*ProveKit doesn't fix your bugs. ProveKit makes the bug class permanently
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
