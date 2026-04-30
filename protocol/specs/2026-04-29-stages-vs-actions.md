# ProvekIt: Stages vs Actions

> Author: shared session 2026-04-29 (T + Claude). Type-level distinction
> between cacheable claims and side-effecting operations.

## Why this spec exists

The workflow runtime today has one contract: `Stage<I, O>`. Every node
in a workflow implements it. The runner caches Stage outputs in the
memento store, keyed by canonicalized input.

This contract assumes pure: same input → same output, output is
content-addressable, output composes by reference into the proof DAG.
For most stages this holds. For some — `openOverlay` (creates a git
worktree on disk), the future `applyBundle`, `runVitestSuite` against
shared filesystem, anything writing to a shared resource — the contract
forces a workaround. `openOverlay`'s current implementation salts its
input with `crypto.randomUUID()` and throws on `deserializeOutput`,
guaranteeing cache misses. That is a smell, not a fix.

The right cut is at the type level. Side-effecting operations are not
Stages. They are a different kind of node — `Action<I, R>` — with a
different contract, a different runner method, and a different DAG
representation.

This spec fixes:
- The `Stage<I, O>` and `Action<I, R>` contracts.
- The runner's `runStage` (cache-aware) and `runAction` (cache-bypassing)
  methods.
- The audit-only memento that records each action invocation.
- The YAML manifest syntax for declaring actions vs nodes.
- The reference language so node inputs cannot accidentally consume
  action resources into binding hashes.
- The migration plan for existing producers.

## The two contracts

```typescript
/**
 * Pure, cacheable. Output is a CLAIM. Composes by reference in the
 * proof DAG. The producer of every memento with verdict ∈ {holds,
 * violated, decayed, undecidable, error}.
 */
interface Stage<I, O> {
  name: string;
  producedBy: string;

  /** Canonicalize input for the propertyHash. */
  serializeInput(input: I): unknown;

  /** Render output as the witness column for memento storage. */
  serializeOutput(output: O): string;

  /** Reconstruct output from witness on cache hit. */
  deserializeOutput(witness: string): O;

  /** The actual work. Only invoked on cache miss. */
  run(input: I): Promise<O>;
}

/**
 * Impure, run-every-time. Output is a RESOURCE handle bound to the
 * call site, not content-addressable. Emits an audit-only memento
 * (write-only — no cache lookup ever happens). Does not compose into
 * the proof DAG; can be REFERENCED from the audit DAG, not from
 * binding hashes.
 */
interface Action<I, R> {
  name: string;
  producedBy: string;

  /**
   * Canonicalize input for the audit memento's binding hash. NOT used
   * for cache lookup. Two calls with the same input produce two
   * separate audit mementos (different `producedAt`, different `cid`).
   */
  serializeInput(input: I): unknown;

  /**
   * Render the resource as a human-readable description for the audit
   * memento's `evidence.body.resourceDescription`. Must NOT include
   * the live resource handle — only metadata sufficient to identify
   * what was produced (e.g., the worktree path, the lock identifier,
   * the file path written).
   */
  describeResource(resource: R): string;

  /** Always invoked. */
  run(input: I): Promise<R>;
}
```

The two contracts differ in three places:

1. `Stage` has `serializeOutput` / `deserializeOutput`; `Action` has
   `describeResource`. Stages cache by witness; Actions never reload
   from witness.
2. `Stage.run` is invoked only on cache miss; `Action.run` is invoked
   always.
3. Stage outputs are claims (verdict-bearing, hashable); Action outputs
   are resources (live handles, not safe to deserialize from witness).

## Runner methods

```typescript
class WorkflowRunner {
  // Existing: cache-aware claim production.
  async runStage<I, O>(
    stage: Stage<I, O>,
    input: I,
    inputCids: string[] = [],
  ): Promise<StageResult<O>>;

  // New: cache-bypassing resource production.
  async runAction<I, R>(
    action: Action<I, R>,
    input: I,
    inputCids: string[] = [],
  ): Promise<ActionResult<R>>;
}

interface StageResult<O> {
  output: O;
  cid: string;             // memento CID
  cacheHit: boolean;       // true when run() was skipped
}

interface ActionResult<R> {
  resource: R;             // live resource handle
  auditCid: string;        // audit memento CID
  // No cacheHit field — Actions never hit cache.
}
```

`runStage` writes a memento with the standard claim envelope (verdict
typically `holds`; if the Stage threw, no memento — the framework
records the error elsewhere).

`runAction` writes an audit memento with:
- `verdict: error` is reserved for Actions that explicitly fail; on
  successful execution the audit memento has `verdict: holds` —
  meaning *the action ran successfully*, not that some property was
  verified. The verdict-as-action-success interpretation is by
  convention; the wrapper schema does not encode the difference, only
  the evidence variant signals it.
- `evidence.kind: action-invocation` (a new standard variant added by
  this spec).
- `evidence.body.resourceDescription`: the `describeResource(resource)`
  output.
- `evidence.body.actionName`: stage's `name`.

The `action-invocation` variant body schema:

```yaml
kind: action-invocation
schema: <CID of the action-invocation schema>
body:
  actionName: string
  resourceDescription: string
  durationMs: number
  exitStatus: optional string
```

Audit mementos are written to the memento store with the standard
wrapper, but consumers walking the proof DAG MUST skip them. They
appear only in audit DAG walks.

## YAML manifest syntax

The manifest declares Stages and Actions in separate top-level blocks.
References are syntactically distinct so a Stage cannot accidentally
consume an Action's resource into its bindingHash.

```yaml
name: bug-fix
cid: bafy...
description: ...

# Stages: cacheable, claim-producing.
nodes:
  - id: intake
    capability: intake
    input: $input

  - id: do-the-work
    capability: do-the-work
    input:
      signal: $node.intake.output
      overlay: $action.open-overlay.resource    # explicit action ref
      invariant: $node.formulate.output

# Actions: side-effecting, run-every-time.
actions:
  - id: open-overlay
    action: open-overlay
    input:
      baseRef: $input.baseRef
      worktreeRoot: $input.worktreeRoot

  - id: cleanup-overlay
    action: close-overlay
    input:
      overlay: $action.open-overlay.resource
    runAfter: $node.do-the-work     # explicit ordering
```

Reference forms:

- `$input.path` — workflow's input.
- `$node.<id>.output` — Stage output. Allowed in: Stage inputs, Action
  inputs.
- `$action.<id>.resource` — Action resource. Allowed in: Stage inputs,
  Action inputs. NOT allowed as a value in any input that participates
  in a propertyHash.

Validation: the manifest parser walks each Stage node's input and
confirms that no `$action.<id>.resource` reference appears inside the
canonicalizable portion of the input — only the runtime-resolvable
portion. In practice, Stage inputs accept Action resources as a
DISTINGUISHED FIELD that the framework strips before computing the
propertyHash.

For example, `do-the-work` declares its input shape as:

```typescript
interface DoTheWorkInput {
  signal: IntentSignal;          // hashable, contributes to propertyHash
  invariant: InvariantClaim;     // hashable, contributes to propertyHash
  overlay: OverlayHandle;        // RESOURCE — excluded from propertyHash
}
```

The Stage's `serializeInput` returns only the hashable fields; the
runtime-resolved `overlay` is passed to `run()` but never enters the
hash. The manifest reference `$action.open-overlay.resource` is
threaded through the runtime call but excluded from the binding hash
by construction.

This is what `do-the-work`'s current spec already documents: only
`overlay.baseRef` enters the hash, never `overlay.worktreePath`. The
Stages-vs-Actions split makes that exclusion *enforceable* via the
type system rather than relying on producer discipline.

## Action ordering and lifecycle

Actions are not part of the proof DAG, but they have ordering
constraints that the runtime must honor:

- An Action's `runAfter` clause specifies a Stage or another Action
  whose execution must complete before this one runs. Default is "no
  constraint" (Action can run as soon as its inputs resolve).
- Cleanup Actions (close handles, drop locks, etc.) typically declare
  `runAfter: <terminal-stage>` so they fire after the work that
  consumed the resource has completed.
- Actions that produce resources consumed by Stages have an implicit
  `runBefore` relationship — the Stage cannot start until its
  resource-input is available.

The runtime topo-sorts Stages and Actions together, with the type
system ensuring cycles between them are impossible.

## Audit-only memento walk

The proof DAG (the structure used for verifying a codebase's
correctness) is the subset of the memento store containing only Stage
mementos. The audit DAG (the structure used for forensics, replay,
incident response) includes both Stage and Action mementos.

```typescript
// Returns only claim mementos (Stage outputs).
walk(cid: string): Memento[]
// Returns ALL mementos including action invocations.
walkWithActions(cid: string): Memento[]
```

The `evidence.kind` field discriminates: any memento with
`kind: action-invocation` is an audit-only memento and is skipped by
`walk`. Consumers needing forensic detail use `walkWithActions`.

## Migration plan

1. **Add Action interface to `src/workflow/types.ts`.** New file or
   extension; existing Stage interface unchanged.
2. **Add `runAction` to `WorkflowRunner`.** Orthogonal to `runStage`;
   no breaking change to existing call sites.
3. **Refactor `openOverlay` from Stage to Action.** Delete the
   `crypto.randomUUID()` salt and the throwing `deserializeOutput`.
   Implement `describeResource(handle)` to return the worktree path.
4. **Update existing tests for `openOverlay`.** Replace
   `runner.runStage(openOverlayStage, …)` with
   `runner.runAction(openOverlayAction, …)`. Existing assertions about
   the Action's resource (`worktreePath`, `baseRef`) still hold.
5. **Identify other side-effecting Stages.** Any stage that mutates
   shared state (filesystem, DB, network) is a candidate. Initial
   audit:
   - `openOverlay` — confirmed Action.
   - `applyBundle` (when implemented) — Action (writes the patch to
     disk).
   - `runVitestSuite` against shared cwd — Action (the test runner
     produces output IS a Stage; the side effect of writing test
     results to disk is an Action).
   - All current Stages 1-7 — confirmed pure (intake, formulate,
     classify, locate, investigate, do-the-work, bundle, recognize,
     generateComplementary, generatePrincipleCandidate). investigate's
     report-write is borderline; the consumer reads the in-memory
     report, not the file, so the file-write is incidental rather than
     load-bearing. Document the side effect but keep as Stage.
6. **Update YAML manifest grammar.** Add the `actions:` block to the
   parser; add `$action.<id>.resource` reference resolution; add the
   ordering directives.
7. **Migrate `bug-fix.workflow.yaml`.** Move `open-overlay` from
   `nodes` to `actions`. Update references in downstream nodes.
8. **Add the `action-invocation` evidence variant** to the standard
   variant set defined in the universal-claim-envelope spec.
9. **Update tests.** Existing 79+ workflow tests stay green. New tests
   cover: action invocation, audit memento writing, mixed
   Stage/Action manifests, walk vs walkWithActions semantics, action
   resource types not contributing to Stage propertyHashes.

## Acceptance test

The split is correct when:

1. `openOverlay` is an Action with no cache-busting hacks, no salt, no
   throwing deserializer. `runAction(openOverlayAction, ...)` always
   creates a fresh worktree; never returns a cached handle.
2. The audit memento for `openOverlay` records the worktree path and
   `producedAt` but is skipped by proof-DAG walks.
3. `do-the-work` consumes `overlay: OverlayHandle` from an action ref
   in YAML; the framework threads the live handle through the runtime
   call but excludes it from the propertyHash.
4. Two runs of the same workflow with the same input but different
   worktree paths produce identical `do-the-work` mementos (because
   only `overlay.baseRef` is hashed).
5. Cycles between Stages and Actions are detected at manifest parse
   time and refused.
6. The TypeScript type system prevents passing an Action where a
   `Stage<I, O>` is expected and vice versa.

When all six hold, the architectural correction has landed and the
"never cache-bust by salt" rule is enforceable at the type level.

## Implementation notes

- `Action<I, R>` lives in `src/workflow/types.ts` next to `Stage<I, O>`.
- `runAction` lives in `src/workflow/runner.ts` next to `runStage`.
- The `action-invocation` evidence schema ships in
  `@provekit/claim-envelope`'s `evidence-schemas/` directory.
- The manifest parser in `src/workflow/manifest.ts` extends to handle
  `actions:` blocks and `$action.<id>.resource` references.
- The walk function in `src/fix/runtime/mementoStore.ts` adds an
  optional `includeActions` flag (default `false`).
- Existing `openOverlay` and its tests are refactored as part of this
  spec's implementation. Other producers stay as Stages until shown
  to mutate shared state.
