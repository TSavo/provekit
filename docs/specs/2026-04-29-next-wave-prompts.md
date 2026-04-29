# ProvekIt: next-wave implementation prompts

> Authored: 2026-04-29. These are complete agent prompts for the next
> implementation wave, ready to dispatch. Each is self-contained. The
> receiving agent should have no prior context on this codebase.

---

## Prompt 1: Diff-driven intent extraction LLM-producer

You are implementing a new Stage producer in the ProvekIt framework.
The framework lives at `/Users/tsavo/provekit`. Working tree is clean.
All 79+ existing tests pass. Do not break them.

### Stakes

This is the load-bearing feature for mainframe enterprise adoption.
ProvekIt's market thesis is "point it at your repo, it reads your
history, it mints mementos." That claim is only true when this Stage
exists. Without it, every IR formula has to be hand-authored, which
means: developer behavior change required, annotation discipline
required, no enterprise buy-in. With it: the framework reads decades
of commit history and extracts IR formula proposals with zero developer
effort. The COBOL-shop-with-50-years-of-history pitch depends on this
Stage running at mine-history scale. If you build it wrong, the cache
miss rate destroys performance (same diff re-mined every run), the
evidence envelope doesn't match the `llm-proposal` variant shape
(downstream cross-validation breaks), or the prompt CID isn't encoded
into the producer-id (a prompt update silently serves stale proposals
from prior runs). All three of these failures are silent.

### Read first (in this order)

1. `src/workflow/types.ts` — the `Stage<TInput, TOutput>` interface.
   You are implementing this contract. The three pure functions
   (`serializeInput`, `serializeOutput`, `deserializeOutput`) plus
   `run()`. READ THE WHOLE FILE before touching anything. The
   `Action<TInput, TResource>` interface is also defined here; do not
   confuse the two. This Stage reads a diff and emits a proposal — that
   is a pure, deterministic operation (same diff hash + same prompt CID
   = same proposal). It is a Stage, not an Action.

2. `src/workflow/producers/intake.ts` — the simplest existing
   LLM-using producer. This is the pattern to follow exactly:
   `makeIntakeStage(deps)` factory, `producedBy` encodes producer
   identity, `serializeInput` returns a canonical object excluding
   runtime-only fields, `serializeOutput`/`deserializeOutput` round-trip
   via JSON.

3. `src/workflow/producers/intake.test.ts` — the test pattern to
   follow. Note the `makeDb()` helper, the `StubLLMProvider` usage, the
   cache-hit test that wraps the LLM and counts calls.

4. `src/workflow/producers/formulate.ts` — a producer with a deps
   object rather than a single arg; use this pattern because your Stage
   has multiple injected dependencies (llm, promptCid).

5. `src/workflow/producers/classify.test.ts` — shows the `vi.hoisted`
   pattern for mocking LLM dependencies. The first three lines after
   the imports are load-bearing:
   ```typescript
   const { classifyMock } = vi.hoisted(() => ({ classifyMock: vi.fn() }));
   vi.mock("../../fix/classify.js", () => ({ classify: classifyMock }));
   ```
   Use the `StubLLMProvider` approach from intake.test.ts instead of
   `vi.mock` if you keep the LLM as a direct dep rather than a module
   call.

6. `src/claimEnvelope/index.ts` (via `git show dcb52ec:src/claimEnvelope/index.ts`)
   and `src/claimEnvelope/types.ts` (via `git show dcb52ec:src/claimEnvelope/types.ts`)
   — the claim envelope module. The `LlmProposalEvidence` variant is
   what your witness must match. Its body shape:
   ```typescript
   {
     llm: string;           // e.g. "claude-opus"
     llmVersion: string;    // e.g. "4-7"
     promptCid: string;     // hex32 CID of the prompt artifact
     proposedIrFormula: string; // serialized IR formula (free-form in v1)
     confidence: number;    // 0..1
     rationale?: string;
   }
   ```
   Note: the claim-envelope module was implemented in commit `dcb52ec`
   but may not be on-disk yet. Check `ls src/claimEnvelope/`. If it
   exists, import from it. If not, look at the commit:
   `git show dcb52ec:src/claimEnvelope/index.ts`. The module exports
   `signEnvelope`, `verifyEnvelopeSignature`, `validateEnvelope`,
   `computeEnvelopeCid`, and all variant types.

7. `docs/specs/2026-04-29-universal-claim-envelope.md` — the
   `llm-proposal` evidence variant section. The `evidence.schema` field
   must be a hex32 CID of the variant schema definition. For v1, you
   can use a deterministic placeholder CID computed from the string
   `"llm-proposal-schema-v1"` with `hashCanonical`.

8. `src/fix/runtime/mementoStore.ts` lines 60-85 — the `hashCanonical`
   function. CRITICAL QUIET PART: `hashCanonical` returns a
   **sha256-prefix-16** (16 hex chars). The memento `cid` is
   sha256-prefix-32 (32 hex chars). These are different lengths. Do not
   confuse them. `hashCanonical` is for binding/property hashes. The
   runner's `writeMemento` computes the memento `cid` itself.

### What to implement

Create `src/workflow/producers/intentFromDiff.ts` and
`src/workflow/producers/intentFromDiff.test.ts`.

#### The types

```typescript
export interface TicketRef {
  id: string;
  url?: string;
  summary?: string;
}

export interface IntentFromDiffInput {
  diff: string;
  commitMessage: string;
  linkedTickets?: TicketRef[];
  hostLanguageHint?: string;
}

export interface IntentProposal {
  proposedIrFormula: string;    // serialized IR formula (free-form in v1)
  confidence: number;           // 0..1
  rationale: string;
  inferredIntent: string;       // natural-language description of what the dev was trying to do
  llm: string;                  // LLM identifier (e.g. "claude")
  llmVersion: string;           // e.g. "opus-4-7"
  promptCid: string;            // CID of the prompt artifact used
}
```

#### The factory

```typescript
export interface MakeIntentFromDiffStageDeps {
  llm: LLMProvider;
  promptCid: string;    // content-hash of the prompt being used
  llmIdentifier: string;  // e.g. "claude-opus@4-7"
  producerVersion?: string;  // default: "intent-from-diff@v1+<llmIdentifier>+<promptCid[:8]>"
}

export function makeIntentFromDiffStage(
  deps: MakeIntentFromDiffStageDeps,
): Stage<IntentFromDiffInput, IntentProposal> { ... }
```

The `producerVersion` must encode the LLM identifier AND a prefix of
the prompt CID. This is what makes a prompt revision invalidate prior
cache entries. If the prompt changes, the promptCid changes, the
producedBy changes, the propertyHash computation changes, and the
cache misses correctly.

#### The cache key

`serializeInput` must return a canonical object covering:
- `diffHash`: `hashCanonical(input.diff)` — NOT the raw diff (which
  can be huge). Hash it.
- `commitMessage`: `input.commitMessage`
- `ticketContent`: the sorted-and-joined ticket summaries, or `null`
- `hostLanguageHint`: `input.hostLanguageHint ?? null`

Do NOT include the raw diff in the serialized input. The diff can be
megabytes; storing it in the binding hash computation is wasteful and
fragile. Hash it first.

#### The LLM prompt

The `run()` method sends one prompt to the LLM. The prompt teaches the
LLM what to extract and what format to produce. For v1, embed the
prompt inline as a template string. The prompt must:

1. Describe the task: given a diff + commit message (+ optional linked
   tickets), extract what property the developer was asserting.
2. Show an example of a good extraction:
   ```
   Diff: +  if denominator == 0 { return Err("...") }
   Commit: "fix divide-by-zero crash"
   Ticket: "INC-2847: calculate() crashes when called with b=0"
   
   Good extraction:
   {
     "inferredIntent": "function calculate must not be called with a zero denominator",
     "proposedIrFormula": "forAll(call: CalculateCall) => call.b !== 0 OR call.returnsError()",
     "confidence": 0.92,
     "rationale": "The guard clause added in the diff directly encodes the invariant..."
   }
   ```
3. Show a bad extraction (what the LLM should NOT do):
   ```
   Bad: { "inferredIntent": "fixed a bug", "proposedIrFormula": "true", "confidence": 0.5 }
   ```
   Bad because: the formula is vacuous; the intent is a tautology that
   names nothing verifiable.

The LLM response must be JSON. Parse it. If it fails to parse, throw
with the raw response included in the error message.

#### Witness storage

`serializeOutput` must produce a JSON string whose shape matches the
`LlmProposalEvidence` body from the universal claim envelope spec. The
acceptance criteria explicitly requires the `llm-proposal` evidence
variant to be correctly populated — a bare `IntentProposal` JSON object
is NOT sufficient.

The `LlmProposalEvidence` body shape (from
`git show dcb52ec:src/claimEnvelope/types.ts`):
```typescript
{
  llm: string;            // e.g. "claude-opus"
  llmVersion: string;     // e.g. "4-7"
  promptCid: string;      // hex32 CID of the prompt artifact
  proposedIrFormula: string;
  confidence: number;     // 0..1
  rationale?: string;
}
```

Map your `IntentProposal` fields to this shape in `serializeOutput`:
```typescript
serializeOutput(output: IntentProposal): string {
  const evidence = {
    llm: output.llm,
    llmVersion: output.llmVersion,
    promptCid: output.promptCid,
    proposedIrFormula: output.proposedIrFormula,
    confidence: output.confidence,
    rationale: output.rationale,
  };
  return JSON.stringify(evidence);
}
```

`deserializeOutput` must reconstruct the full `IntentProposal` from
this stored JSON. The `inferredIntent` field lives on `IntentProposal`
but NOT in `LlmProposalEvidence`. Store it in `rationale` (concatenated
or as a suffix) or add it as an extension field inside the serialized
JSON. Choose one approach and document it in a comment. Round-trip must
be exact: `deserializeOutput(serializeOutput(x))` must reproduce `x`.

### Good vs bad

**Good** — the cache key does not include the raw diff:
```typescript
serializeInput(input) {
  return {
    diffHash: hashCanonical(input.diff),
    commitMessage: input.commitMessage,
    ticketContent: input.linkedTickets
      ? input.linkedTickets.map(t => t.summary ?? t.id).sort().join("|")
      : null,
    hostLanguageHint: input.hostLanguageHint ?? null,
  };
}
```

**Bad** — the cache key includes the raw diff:
```typescript
serializeInput(input) {
  return { diff: input.diff, commitMessage: input.commitMessage };
}
```
Why bad: the serialized input is hashed for the property hash. A raw
diff can be megabytes. More importantly, if the diff has trailing
whitespace variations between environments, the hash changes and the
cache misses spuriously.

**Good** — producer identity encodes the prompt CID:
```typescript
const producedBy = deps.producerVersion ??
  `intent-from-diff@v1+${deps.llmIdentifier}+${deps.promptCid.slice(0, 8)}`;
```

**Bad** — producer identity is static regardless of prompt:
```typescript
const producedBy = "intent-from-diff@v1";
```
Why bad: if you update the LLM prompt, old cache entries are still
served. The LLM was asked a different question but the answer it gave
last time comes back. Stale proposals are worse than no proposals
because the caller trusts them.

### Quiet parts

- `hashCanonical` returns 16 hex chars. `computeCid` (in mementoStore)
  returns 32 hex chars. Never pass a raw diff into `hashCanonical`
  expecting it to act as a full content hash — it does, but the 16-hex
  truncation means prefix collisions are possible for adversarial inputs.
  For non-adversarial diffs, it's fine for v1.

- This is a `Stage`, not an `Action`. A diff-read is not a side effect.
  The LLM call is the work; the result is deterministic given the same
  prompt + same input. The runner handles the cache layer.

- `StubLLMProvider` (from `src/fix/types.ts` or `src/index.ts`)
  matches on substring of the prompt text. Your tests must key the
  stub responses on a recognizable substring of the prompt your `run()`
  method sends. Look at how `intake.test.ts` keys on "Bug report".

- Cross-validation is downstream, not in this Stage. This Stage emits
  one proposal. A second Stage with a different LLM emits another.
  The framework's `findAll`/cross-validation surfaces disagreement.
  Do not implement cross-validation here.

- `linkedTickets` is optional and can be empty. The `run()` prompt
  should degrade gracefully: if there are no tickets, the section of
  the prompt mentioning them is omitted or marked "none."

- The LLM response MUST be valid JSON. Add a try/catch around
  `JSON.parse`. If it fails, rethrow with the raw LLM response in the
  message so test failures are debuggable.

### Cut list

Do NOT implement:
- Git log walking or ticket-system API calls — those are caller
  responsibilities; the inputs arrive pre-fetched.
- Cross-validation logic — downstream concern; out of scope.
- Fine-tuned model inference — use the injected `LLMProvider`.
- Actual IR formula parsing or validation — the formula is a string
  in v1; validation is downstream.
- Any change to `intake.ts` or any other existing producer.
- Full `ClaimEnvelope` wrapper construction or signature logic — your
  Stage uses `writeMemento` which stores the witness column. The
  claim-envelope types define the SHAPE of your witness, not a wrapper
  you must construct.

### Verify

```bash
cd /Users/tsavo/provekit
npx vitest run src/workflow/producers/intentFromDiff.test.ts
```

Tests that must pass:
1. Cache hit on identical inputs — LLM called once; second run returns
   `cacheHit: true` with the same `cid`.
2. Different diff → different memento (different `cid`).
3. Different commit message → different memento.
4. The `output.proposedIrFormula` and `output.confidence` fields are
   populated from the stub LLM response.
5. A different `promptCid` in deps → different `producedBy` → different
   property hash → cache miss (cannot reuse prior entry).

Also run the full suite to confirm no regressions:
```bash
npx vitest run
```
All 79+ tests must still pass.

### Commit

Single commit, conventional format:

```
feat(workflow): intent-from-diff LLM-producer Stage

Factory makeIntentFromDiffStage(deps) caches on (diff hash, commit
message, ticket content, hostLanguageHint). Producer identity encodes
LLM identifier + prompt CID prefix so prompt revision invalidates
prior cache entries. Stub-LLM tests cover cache hit, cache miss on
input change, cache miss on prompt revision.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

## Prompt 2: YAML manifest extension for `actions:` blocks and kit loading

You are extending the ProvekIt workflow manifest parser and runner.
The framework lives at `/Users/tsavo/provekit`. Working tree is clean.
All 79+ existing tests pass. Do not break them.

### Stakes

The manifest parser currently only understands `nodes:` blocks — pure,
cacheable Stages. The Stages-vs-Actions spec adds `actions:` blocks for
side-effecting operations (creating worktrees, applying patches, locking
resources). Without this extension, Actions have no YAML representation
and the bug-fix workflow cannot declare `open-overlay` as an Action in
its manifest — the caller is forced to hard-code the action call in TS,
bypassing the manifest runtime entirely.

The second piece — kit loading via `.provekit/kits.lock` — is the
reproducibility guarantee. Same lockfile + same code = same property
hashes across machines. Without it, two developers running `provekit
prove` on the same repo can produce mementos with different kit CIDs
because their installed kits differ.

If you get the reference language wrong (accidentally allowing
`$action.<id>.output` instead of `$action.<id>.resource`), downstream
Stages can consume action resources into their binding hashes, which
breaks the invariant that two runs with the same code inputs produce
the same Stage memento regardless of which worktree was opened. The
spec is explicit on this; the validation must be enforced in code.

### Read first (in this order)

1. `src/workflow/manifest.ts` — READ THE ENTIRE FILE before touching
   anything. The current parser, the reference language, the
   `parseReference` function, the topo-sort, the `runManifest` runner.
   You are extending all of these.

2. `src/workflow/manifest.test.ts` — READ ALL TESTS. Your changes must
   not break any existing test. New tests extend this file.

3. `docs/specs/2026-04-29-stages-vs-actions.md` — the "YAML manifest
   syntax" section and the "Reference forms" table. The key rules:
   - `$action.<id>.resource` — allowed in Stage inputs and Action inputs,
     but ONLY in a DISTINGUISHED FIELD that the runner strips before
     the Stage's `serializeInput` call.
   - `$action.<id>.output` — INVALID. Actions do not have outputs in the
     Stage sense. The manifest parser must reject this form.
   - `$node.<id>.output` — the existing form for Stage outputs. Unchanged.
   - The topo-sort must handle Stage→Stage, Stage→Action, Action→Stage,
     Action→Action edges. Cycles forbidden.

4. `docs/specs/2026-04-29-per-language-kit-standard.md` — the "Kit
   registry" section and the `.provekit/kits.lock` format:
   ```yaml
   typescript: { version: "0.5.2", cid: hex32 }
   rust: { version: "0.1.0", cid: hex32 }
   ```
   The lockfile pins kit versions. The manifest parser must load it
   if present at `.provekit/kits.lock` relative to the project root.

5. `src/workflow/types.ts` — the `Action<TInput, TResource>` interface
   and `ActionResult<TResource>`. Actions are run via
   `runner.runAction()`, not `runner.runStage()`.

6. `src/workflow/runner.ts` — the existing `runStage` method and the
   new `runAction` method (implemented in commit `c5aa07c`). Check
   whether it's on-disk: `ls src/workflow/runner.ts`. If the file
   exists, read it to see how `runAction` is called. The runner
   already has this method.

7. `src/workflow/registry.ts` — the `ProducerRegistry` and
   `InMemoryRegistry`. You will need to extend the registry concept
   to support Action registration alongside Stage registration, or
   use a parallel registry structure for Actions.

### What to implement

Extend `src/workflow/manifest.ts` and `src/workflow/manifest.test.ts`.
You may also need to extend `src/workflow/registry.ts` for action
registration.

#### New manifest shape

```yaml
name: bug-fix
cid: bafy...
description: ...

# Existing: Stages (cacheable, claim-producing).
nodes:
  - id: intake
    capability: intake
    input: $input

  - id: do-the-work
    capability: do-the-work
    input:
      signal: $node.intake.output
      overlay: $action.open-overlay.resource   # action resource ref
      invariant: $node.formulate.output

# New: Actions (side-effecting, run-every-time).
actions:
  - id: open-overlay
    action: open-overlay                       # action capability name
    input:
      baseRef: $input.baseRef
      worktreeRoot: $input.worktreeRoot

  - id: cleanup-overlay
    action: close-overlay
    input:
      overlay: $action.open-overlay.resource
    runAfter: $node.do-the-work               # ordering constraint
```

#### New types to add to `manifest.ts`

```typescript
export interface ActionSpec {
  id: string;
  action: string;           // action capability name
  input: InputSpec;
  runAfter?: string;        // optional: "$node.<id>" or "$action.<id>"
}

export interface WorkflowManifest {
  // ... existing fields ...
  nodes: NodeSpec[];
  actions?: ActionSpec[];   // NEW: optional, defaults to []
  output: InputRef;
}
```

#### Reference language extension

The `parseReference` function currently handles `$input` and `$node`
roots. Extend it to handle `$action`:

```typescript
type ParsedRef =
  | { kind: "input"; path: string[] }
  | { kind: "node"; nodeId: string; field: string; path: string[] }
  | { kind: "action"; actionId: string; field: string; path: string[] };
```

Validation rules for `$action.<id>.<field>`:
- `field` MUST be `"resource"`. `$action.x.output` is an error:
  "action references must use .resource, not .output".
- The action `id` MUST be declared in `manifest.actions`.
- A `$action.<id>.resource` reference appearing inside a Stage node's
  input is only valid in a DISTINGUISHED FIELD. The enforcement mechanism
  is the Stage's own `serializeInput`: Stage authors must omit resource
  fields from the return value of `serializeInput`, because anything
  returned by `serializeInput` is hashed into the propertyHash. The
  TypeScript type system (the fact that `serializeInput` returns
  `unknown`, not the full input type) is the constraint. The manifest
  parser cannot enforce this at parse time — it does not know which
  input fields `serializeInput` will include. Document this clearly in
  a comment in `manifest.ts`: "Resource fields from $action references
  are passed to run() but must NOT appear in serializeInput() return
  values. This is enforced by Stage author discipline, not by the
  parser." The parser validates that action refs resolve to declared
  action ids; the runner threads them through run() without hashing.

#### Topo-sort extension

The existing `topoSort` handles only Stages. Extend it to handle a
mixed graph of Stages and Actions:

```typescript
export function topoSort(
  nodes: NodeSpec[],
  actions: ActionSpec[],
): Array<{ kind: "node"; spec: NodeSpec } | { kind: "action"; spec: ActionSpec }>;
```

Edge rules:
- A Stage referencing `$node.<id>.output` depends on that Stage.
- A Stage referencing `$action.<id>.resource` depends on that Action.
- An Action referencing `$node.<id>.output` depends on that Stage.
- An Action referencing `$action.<id>.resource` depends on that Action.
- An Action with `runAfter: "$node.<id>"` depends on that Stage.
- An Action with `runAfter: "$action.<id>"` depends on that Action.
- Cycles between Stages and Actions are detected and rejected.

#### `runManifest` extension

`runManifest` must call `runner.runAction()` for Action entries and
`runner.runStage()` (via `registry.request()`) for Stage entries. The
resource returned by `runAction` is stored in a separate `resources`
map (parallel to the existing `records` map for Stage outputs) and is
threaded through when a Stage's input resolves an `$action.<id>.resource`
reference.

The resource is NOT included in `collectInputCids` — action resources
do not contribute to proof DAG edges. Only the audit CID from
`ActionResult.auditCid` is available if needed for forensics.

#### Kit lockfile loading

Add a function to load the kit lockfile:

```typescript
export interface KitLock {
  [kitName: string]: { version: string; cid: string };
}

export function loadKitsLock(projectRoot: string): KitLock | null;
```

Load `.provekit/kits.lock` relative to `projectRoot`. Parse the YAML.
Return `null` if the file does not exist. Throw with a clear message
if it exists but is malformed. The lockfile is not used by `runManifest`
itself in v1 — expose it so callers can read it and verify their
installed kit CIDs match before running.

### Good vs bad

**Good** — action ref validation rejects `.output`:
```typescript
if (parsed.kind === "action" && parsed.field !== "resource") {
  throw new Error(
    `action reference "$action.${parsed.actionId}.${parsed.field}" is invalid — ` +
    `action references must end in .resource (actions do not have .output)`
  );
}
```

**Bad** — the parser only validates that action ids are declared but
lets any field through:
```typescript
// Wrong: allows $action.x.output which breaks the invariant
if (parsed.kind === "action" && !actionIds.has(parsed.actionId)) {
  throw new Error(`undeclared action "${parsed.actionId}"`);
}
```
Why bad: a Stage consuming `$action.open-overlay.output` would try to
hash the action's "output" (which doesn't exist) into the binding hash.
Silent breakage. The spec is explicit: the only valid action reference
field is `resource`.

**Good** — topo-sort handles both kinds:
```typescript
for (const ref of collectReferences(action.input)) {
  const parsed = parseReference(ref);
  if (parsed.kind === "node") {
    // stage must complete before this action
    dependsOn.get(`action:${action.id}`)!.add(`node:${parsed.nodeId}`);
  }
  if (parsed.kind === "action") {
    dependsOn.get(`action:${action.id}`)!.add(`action:${parsed.actionId}`);
  }
}
```

**Bad** — topo-sort only processes nodes, ignores actions:
```typescript
for (const node of nodes) {
  for (const ref of collectReferences(node.input)) { ... }
}
// Actions not processed — action ordering constraints are silently dropped
```

### Quiet parts

- The existing `assertReferenceValid` function rejects any `$node.<id>`
  reference that doesn't end in `.output`. Your new validation for
  `$action.<id>` must reject any field that isn't `.resource`. The
  parallel structure is deliberate — match it.

- Stage resources from Actions are NOT included in `collectInputCids`.
  Only Stage outputs contribute to the memento DAG. The audit DAG tracks
  Actions separately. The `runManifest` runner must not pass action
  `auditCid`s into Stage `inputCids`.

- The `runAfter` clause on an Action is an ordering constraint, not a
  data dependency. The Action does not consume the Stage's output — it
  just must not start until the Stage has completed. Implement this by
  adding a dependency edge in the topo-sort but NOT resolving the
  Stage's output into the Action's input.

- The existing manifest `output` field references a node (Stage output).
  An Action's resource cannot be the workflow's terminal output — that
  would make the workflow's output an uncacheable live handle. Validate
  that `manifest.output` points to a Stage node, not an Action.

- Kit lockfile: do not fail if `.provekit/kits.lock` doesn't exist.
  Most current tests run without it. The function returns `null` in
  that case; callers decide what to do.

### Cut list

Do NOT implement:
- Remote kit registry fetch — just the lockfile loading.
- Kit signature verification — that's Prompt 3's territory.
- Any change to existing Stage producers' input shapes.
- Execution of Action cleanup logic (that's the caller's orchestration).
- Any change to how `runStage` works.

### Verify

```bash
cd /Users/tsavo/provekit
npx vitest run src/workflow/manifest.test.ts
npx vitest run  # full suite, all 79+ must pass
```

New test cases to add to `manifest.test.ts`:
1. A manifest with an `actions:` block parses without error.
2. An action referenced by `$action.<id>.resource` in a Stage input
   resolves correctly.
3. `$action.<id>.output` is rejected with a clear error message.
4. An undeclared action id in a reference is rejected.
5. A cycle between a Stage and an Action is detected.
6. `runAfter` on an Action creates the correct ordering (the Action
   runs after the referenced Stage).
7. `loadKitsLock` returns `null` when the file doesn't exist.
8. `loadKitsLock` parses a valid lockfile and returns the version/cid map.
9. The workflow `output` field pointing at an action id is rejected.

### Commit

```
feat(workflow): manifest actions blocks, action refs, kit lockfile loading

Extends WorkflowManifest with optional actions: block. Adds ActionSpec
type, $action.<id>.resource reference form (rejects .output), mixed
Stage/Action topo-sort, runManifest action dispatch via runner.runAction,
and loadKitsLock for .provekit/kits.lock. Existing 79+ tests green;
new tests cover action grammar, validation, ordering, and lockfile.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

## Prompt 3: Producer-signature key management

You are implementing cryptographic key management for producer signatures
in the ProvekIt framework. The framework lives at `/Users/tsavo/provekit`.
Working tree is clean. All 79+ existing tests pass. Do not break them.

### Stakes

When mementos move between machines — developer → CI → deployment →
audit — the consumer needs to verify that the memento was actually
produced by who it claims. Without producer signatures, an attacker who
compromises the memento store can swap any verdict to `holds` without
detection. Producer signatures are the mechanism that makes ProvekIt's
trust-but-verify architecture non-repudiable: a producer who signed a
memento cannot later deny it; an attacker forging a memento must also
forge the producer's signature; consumers verify signatures in
microseconds.

This module is the cryptographic plumbing that makes swarm distribution
safe. Without it, the seven-tier trust capture (developer → git host →
CI → deployment → audit → package registry → dependency manager) reduces
to "hope the store wasn't tampered with." With it, every tier's
verification is a signature check against a content-addressed public key,
and key compromise is detectable through the revocation chain.

If you get key rotation wrong (fail to walk the rotation chain),
consumers using a producer's old key reject valid new mementos as
forgeries, breaking cross-validation in multi-producer deployments. If
you get revocation wrong (fail to check the revoke chain before
`producedAt`), revoked compromised keys keep signing mementos consumers
accept. Both failures are silent and catastrophic.

### Read first (in this order)

1. `src/claimEnvelope/sign.ts` (via `git show dcb52ec:src/claimEnvelope/sign.ts`)
   — the `signEnvelope` and `verifyEnvelopeSignature` functions already
   implemented by the parallel claim-envelope agent. Your module wraps
   these; it does NOT re-implement them. Read the file to understand the
   exact function signatures. The claim-envelope module may not be
   on-disk yet (`ls src/claimEnvelope/`). If it isn't, read it from
   git: `git show dcb52ec:src/claimEnvelope/sign.ts`.

2. `src/claimEnvelope/types.ts` (via `git show dcb52ec:src/claimEnvelope/types.ts`)
   — the `ClaimEnvelope` type. Your `publishPublicKey` and rotation/
   revocation functions emit mementos with specific `evidence.kind`
   values that are NOT yet in the standard variants. You will add them.

3. `src/claimEnvelope/index.ts` (via `git show dcb52ec:src/claimEnvelope/index.ts`)
   — the full export list. Understand what the claim-envelope module
   already provides before adding anything.

4. `docs/specs/2026-04-29-universal-claim-envelope.md` — the
   "Producer-signature scheme (v1)" section. The exact scheme:
   - ed25519 keypair per producer-id.
   - Public key published as a content-addressed artifact
     (`kind: "producer-public-key"` memento).
   - Key rotation: a new public key published with a rotation memento
     referencing the old key's CID.
   - Revocation: a revoke memento referencing the compromised key's CID.
   - Consumers walk the rotation chain to validate signatures against
     historically-current keys.

5. `src/fix/runtime/mementoStore.ts` — `writeMemento`, `findMemento`,
   `hashCanonical`. Your key mementos are written to the same memento
   store. Read the function signatures and the `Memento` type to
   understand what you're writing.

6. `src/workflow/types.ts` — the `Stage<TInput, TOutput>` interface.
   The `publishPublicKey` function is a producer of key mementos.
   These mementos go through `writeMemento` directly (not via
   `runStage`) because key publication is not a cacheable Stage in
   the normal sense — each publication is an intentional act.

### What to implement

Create `src/producerKeys/index.ts` and `src/producerKeys/index.test.ts`.
The module is small and focused. Six functions:

```typescript
import { KeyObject } from "node:crypto";
import type { Db } from "../db/index.js";

export interface ProducerKeypair {
  publicKey: KeyObject;
  privateKey: KeyObject;
  publicKeyCid: string;    // CID of the published key memento (once published)
}

/**
 * Generate a fresh ed25519 keypair.
 * For tests, pass a deterministic seed via options.seed to get
 * reproducible keypairs.
 */
export function generateKeypair(options?: { seed?: Buffer }): ProducerKeypair;

/**
 * Publish the public key as a content-addressed memento with
 * kind: "producer-public-key". The memento's bindingHash encodes
 * the producerId; the propertyHash encodes the public key bytes.
 * Returns the CID of the published key memento.
 *
 * The memento's witness is the DER-encoded public key, base64.
 * The producedBy is "provekit-key-mgmt@v1".
 */
export function publishPublicKey(
  producerId: string,
  keypair: ProducerKeypair,
  db: Db,
): string;

/**
 * Walk the rotation chain to find the current valid public key for
 * a producerId. Throws if no key is found. Returns null if the key
 * has been revoked (a revoke memento exists for the current key).
 *
 * Algorithm:
 * 1. Find the most recent "producer-public-key" memento for this producerId.
 * 2. If a "producer-key-rotation" memento exists that references this
 *    key's CID, follow the rotation to the newer key.
 * 3. Repeat until no rotation memento exists for the current key.
 * 4. Check for a "producer-key-revoke" memento referencing the final key.
 * 5. If found, return null (key is revoked). Otherwise return the key.
 */
export function loadProducerKey(
  producerId: string,
  db: Db,
): KeyObject | null;

/**
 * Sign a claim envelope using the private key.
 * This is a thin wrapper around signEnvelope from the claim-envelope module.
 */
export function signMemento(
  envelope: ClaimEnvelopeForSigning,
  privateKey: KeyObject,
): string;

/**
 * Verify a claim envelope's signature by loading the producer's key
 * from the rotation chain and checking the signature.
 * Returns a detailed result including rotation chain provenance.
 */
export interface VerifyResult {
  valid: boolean;
  reason?: string;
  keyCid?: string;          // CID of the key that verified the signature
  keyRotationDepth?: number; // 0 = original key, 1 = first rotation, etc.
  revoked?: boolean;
}

export function verifyMemento(
  envelope: ClaimEnvelope,
  db: Db,
): VerifyResult;

/**
 * Publish a key rotation memento. The new key replaces the old key.
 * Consumers walking the rotation chain from the old key's CID will
 * arrive at the new key.
 *
 * The rotation memento's witness encodes:
 *   { oldKeyCid: string, newPublicKey: string (base64 DER) }
 */
export function rotateKey(
  producerId: string,
  oldKeypair: ProducerKeypair,
  newKeypair: ProducerKeypair,
  db: Db,
): string; // returns CID of the rotation memento

/**
 * Publish a revocation memento for a compromised key. After this call,
 * verifyMemento will return { valid: false, revoked: true } for any
 * memento signed with the compromised key.
 *
 * The revocation memento's witness encodes:
 *   { compromisedKeyCid: string, reason?: string }
 */
export function revokeKey(
  producerId: string,
  compromisedKeypair: ProducerKeypair,
  signingKeypair: ProducerKeypair,
  db: Db,
  reason?: string,
): string; // returns CID of the revoke memento
```

### Memento shapes for key operations

Key publication and rotation mementos are stored in the same memento
store as proof mementos, but with a distinguished `producedBy` of
`"provekit-key-mgmt@v1"` so they're identifiable in walks.

**Public key memento:**
- `bindingHash`: `hashCanonical({ producerId, kind: "producer-public-key" })`
- `propertyHash`: `hashCanonical({ publicKeyDer: <base64 DER> })`
- `verdict`: `"holds"` (the key exists and is valid)
- `witness`: JSON string `{ kind: "producer-public-key", producerId, publicKeyDer: <base64> }`
- `producedBy`: `"provekit-key-mgmt@v1"`

**Rotation memento:**
- `bindingHash`: `hashCanonical({ producerId, kind: "producer-key-rotation", oldKeyCid })`
- `propertyHash`: `hashCanonical({ newPublicKeyDer: <base64 DER> })`
- `verdict`: `"holds"`
- `witness`: JSON `{ kind: "producer-key-rotation", producerId, oldKeyCid, newPublicKeyDer: <base64> }`
- `inputCids`: `[oldKeyMemento.cid]` — DAG edge back to the old key

**Revocation memento:**
- `bindingHash`: `hashCanonical({ producerId, kind: "producer-key-revoke", compromisedKeyCid })`
- `propertyHash`: `hashCanonical({ reason: reason ?? "" })`
- `verdict`: `"violated"` — the key's integrity claim is violated
- `witness`: JSON `{ kind: "producer-key-revoke", producerId, compromisedKeyCid, reason }`
- `inputCids`: `[compromisedKeyMemento.cid]`

### Worked example: rotation chain walk

```
Original key published:       memento CID = "aaa..."
  |
Rotation memento published:   bindingHash encodes oldKeyCid = "aaa..."
  newPublicKeyDer = <rotated key>
  inputCids = ["aaa..."]
  CID = "bbb..."
  |
No further rotation exists.

loadProducerKey("my-producer", db):
  1. Find all mementos with bindingHash = hashCanonical({ producerId: "my-producer", kind: "producer-public-key" })
     → finds "aaa..."
  2. Look for a rotation memento with bindingHash = hashCanonical({ producerId: "my-producer", kind: "producer-key-rotation", oldKeyCid: "aaa..." })
     → finds "bbb..."
  3. Extract newPublicKeyDer from "bbb...".witness
  4. Look for another rotation from "bbb..."
     → not found
  5. Check for revoke memento referencing "bbb..."
     → not found
  6. Return KeyObject.from(newPublicKeyDer)
```

```
Revocation after compromise:
  revokeKey("my-producer", compromisedPair, signingPair, db)
  → writes revoke memento with bindingHash = hashCanonical({ producerId: "my-producer", kind: "producer-key-revoke", compromisedKeyCid: "bbb..." })

loadProducerKey("my-producer", db):
  ... walks to "bbb..." as above ...
  5. Check for revoke memento referencing "bbb..."
     → found
  6. Return null (key revoked)
```

### Good vs bad

**Good** — rotation chain walk is iterative, follows the chain:
```typescript
function walkRotationChain(producerId: string, currentCid: string, db: Db): string {
  const rotationBindingHash = hashCanonical({
    producerId, kind: "producer-key-rotation", oldKeyCid: currentCid
  });
  const rotation = findMemento(db, { bindingHash: rotationBindingHash, propertyHash: ... });
  if (!rotation) return currentCid;  // no further rotation
  const { newPublicKeyDer } = JSON.parse(rotation.witness!);
  const newCid = rotation.cid!;
  return walkRotationChain(producerId, newCid, db);  // follow the chain
}
```

**Bad** — only looks for one rotation, misses deeper chains:
```typescript
const rotation = findMemento(db, { ... });  // only looks once
if (rotation) {
  const { newPublicKeyDer } = JSON.parse(rotation.witness!);
  return KeyObject.from(Buffer.from(newPublicKeyDer, "base64"), { type: "spki", format: "der" });
}
return originalKey;  // wrong if there were further rotations
```
Why bad: if the producer rotated twice, the consumer gets the first-
rotation key, which may have itself been revoked. Signature verification
fails for correctly-signed mementos.

**Good** — test uses deterministic seed for reproducible keypairs:
```typescript
it("sign/verify roundtrip", () => {
  const keypair = generateKeypair({ seed: Buffer.alloc(32, 0x42) });
  // ... same keypair every run
});
```

**Bad** — test uses `generateKeypair()` with no seed, gets a random
keypair every run. This still passes but makes debugging harder and
can mask non-determinism in other parts of the test.

### Quiet parts

- Node's `crypto.generateKeyPairSync('ed25519')` does not accept a
  seed directly. To generate a deterministic keypair from a 32-byte
  seed, use:
  ```typescript
  import { createPrivateKey, createPublicKey } from "node:crypto";

  // seed must be exactly 32 bytes for ed25519
  const privateKey = createPrivateKey({
    key: seed,
    format: "raw",
    type: "ed25519",   // note: this is the "type" here, not the curve
  });
  const publicKey = createPublicKey(privateKey);
  ```
  This works in Node 16+. The `format: "raw"` path for ed25519 private
  keys accepts the 32-byte seed directly (not the 64-byte expanded form).
  In tests, use `Buffer.alloc(32, 0x42)` or similar as the seed so the
  keypair is identical on every run. This is required for test
  reproducibility — tests that generate a fresh random keypair and then
  call `loadProducerKey` to retrieve it are fragile in CI and produce
  different memento CIDs on every run.

- `findMemento` looks up by `(bindingHash, propertyHash)`. For the
  rotation chain walk, you know the `bindingHash` (it encodes the
  old key CID) but you do NOT know the `propertyHash` in advance
  (it encodes the new key, which you haven't fetched yet). Look at
  `mementoStore.ts` to see if there's a `findAll` by bindingHash.
  If not, you may need to add one or query the DB directly via Drizzle.
  CRITICAL: do NOT use raw SQL — use the repository pattern with Drizzle
  (see the project's memory instruction: "No raw SQL — use repos").

- `signEnvelope` from the claim-envelope module takes a `SignableEnvelope`
  and a private key. For signing key mementos (publishing, rotation,
  revocation), you call `signEnvelope` on the key memento's envelope.
  But the claim-envelope module's `ClaimEnvelope` type has specific
  required fields (`schemaVersion`, `bindingHash`, `propertyHash`, etc.).
  The key mementos you write via `writeMemento` use the simpler
  `Memento` type from `mementoStore.ts`. For v1, you do not need to
  bridge these two memento representations — the key mementos use the
  simpler format; signing of proof mementos is done by the Stage
  producers after this module provides them with the private key.

- ed25519 in Node stdlib: `crypto.generateKeyPairSync('ed25519')` returns
  `{ publicKey: KeyObject, privateKey: KeyObject }`. Signing:
  `crypto.sign(null, data, privateKey)`. Verifying:
  `crypto.verify(null, data, publicKey, signature)`. Algorithm is `null`
  because ed25519 encodes the algorithm in the key itself. This is
  exactly what the claim-envelope's `sign.ts` already uses — match it.

- The `findAll` pattern for rotation chain: you need to find a memento
  by bindingHash alone (without knowing propertyHash). Check
  `src/db/schema/verifications.ts` for the table schema and add a
  `findMementoByBindingHash` helper to `mementoStore.ts` if it doesn't
  exist. This is a legitimate addition to the store module; add it there,
  not in your key-management module.

### Cut list

Do NOT implement:
- HSM-backed signing (forward-compatible via signature-prefix-byte,
  but out of scope for v1).
- Multi-sig producers (separate spec, not this one).
- Integration with any external CA or PKI.
- Automatic key rotation on schedule — this is an API; callers decide
  when to rotate.
- Any change to how Stage producers sign their output mementos — that
  integration is the producer's responsibility after this module ships.
- Swarm-distribution of public keys — keys are local to the DB in v1.

### Verify

```bash
cd /Users/tsavo/provekit
npx vitest run src/producerKeys/index.test.ts
npx vitest run  # full suite, all 79+ must pass
```

Tests that must pass:
1. `generateKeypair()` returns a keypair with `publicKey` and
   `privateKey` as `KeyObject` instances.
2. `publishPublicKey` writes a memento to the DB and returns a CID.
3. `loadProducerKey` finds the published key and returns a `KeyObject`.
4. `signMemento` + `verifyMemento` round-trip: a signed envelope
   verifies correctly against the published key.
5. Rotation: after `rotateKey`, `loadProducerKey` returns the NEW key,
   and `verifyMemento` on a memento signed with the new key returns
   `{ valid: true, keyRotationDepth: 1 }`.
6. Revocation: after `revokeKey` on the current key, `loadProducerKey`
   returns `null` and `verifyMemento` returns `{ valid: false, revoked: true }`.
7. Tampered signature: flip one byte in the signature and confirm
   `verifyMemento` returns `{ valid: false }`.
8. Unknown producer: `loadProducerKey` for a producerId with no
   published key throws a clear error.

### Commit

```
feat(producerKeys): ed25519 key management for producer signatures

generateKeypair, publishPublicKey, loadProducerKey, signMemento,
verifyMemento, rotateKey, revokeKey. Rotation chain walk follows the
chain iteratively; revoke mementos use verdict:violated to signal
compromise. Tests cover sign/verify roundtrip, rotation (depth 1),
revocation, and tampered-signature rejection.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```
