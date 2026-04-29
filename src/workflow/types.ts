/**
 * Workflow primitive — the layer above the memento store.
 *
 * Spec: docs/specs/2026-04-29-workflows-as-first-class-primitive.md
 *
 * A Stage is a single unit of work: hash its input, look up the memento
 * store, return the cached output if it exists, otherwise run and cache.
 * A Workflow composes stages — passing one stage's CID as the next
 * stage's inputCids. Work-skipping cascades: if all upstream stages hit
 * cache, the downstream input hash is unchanged, so it also hits.
 *
 * The Stage contract is deliberately small. Three pure functions
 * (serializeInput, serializeOutput, deserializeOutput) plus run().
 * The framework hashes inputs, stores witnesses, reconstructs outputs.
 * The producer (engine, LLM, callable) shows up only in run().
 */

/**
 * Side-effecting unit of work. Runs every time; never cached. Output
 * is a RESOURCE handle bound to the call site, not content-addressable.
 *
 * Contrast with Stage<I, O>:
 * - Stage: pure, cacheable, output is a CLAIM that composes by reference
 *   in the proof DAG. Verdict-bearing.
 * - Action: impure, run-every-time, output is a RESOURCE handle. Audit-
 *   only memento (write-only — no cache lookup ever happens). Does not
 *   compose into binding hashes downstream.
 *
 * Spec: docs/specs/2026-04-29-stages-vs-actions.md
 */
export interface Action<TInput, TResource> {
  /** Action identity within a workflow. */
  name: string;

  /** Producer identity for audit mementos. */
  producedBy: string;

  /**
   * Canonicalize input for the audit memento. NOT used for cache
   * lookup — Actions always run. Two invocations with the same input
   * produce two separate audit mementos.
   */
  serializeInput(input: TInput): unknown;

  /**
   * Render the resource as a human-readable description for the audit
   * memento's witness. Must NOT include the live resource handle —
   * only metadata sufficient to identify what was produced (the
   * worktree path, the lock identifier, the file path written).
   */
  describeResource(resource: TResource): string;

  /** Always invoked. */
  run(input: TInput): Promise<TResource>;
}

export interface ActionResult<TResource> {
  /** The live resource handle. */
  resource: TResource;
  /** The CID of the audit-only memento recorded for this invocation. */
  auditCid: string;
}

export interface Stage<TInput, TOutput> {
  /**
   * Stage identity within a workflow. Used as part of the binding
   * hash so two stages doing different work on the same input get
   * separate cache slots.
   */
  name: string;

  /**
   * Producer identity for the memento. Engines/LLMs/version pinned.
   * Same stage with two producers gets two rows in the table —
   * cross-validation surfaces disagreements.
   */
  producedBy: string;

  /**
   * Reduce the input to its content-hashable shape. The framework
   * canonicalizes (sorted keys, stable order) before hashing. Any
   * field that doesn't affect the output should be excluded here.
   */
  serializeInput(input: TInput): unknown;

  /**
   * Render the output for storage in the memento's witness column.
   * Must round-trip with deserializeOutput.
   */
  serializeOutput(output: TOutput): string;

  /**
   * Reconstruct the output from its witness. The cache-hit path
   * uses this — we never re-run when the memento is found.
   */
  deserializeOutput(witness: string): TOutput;

  /**
   * The actual work. Only invoked on cache miss.
   */
  run(input: TInput): Promise<TOutput>;
}

export interface Workflow {
  /**
   * Workflow identity for telemetry, audit, and the binding hash.
   * Two workflows with the same stage name get separate cache slots.
   */
  name: string;

  /**
   * Content hash of the workflow definition. When the workflow itself
   * changes (different stage sequence, different stage producers),
   * the cid changes, invalidating the entire cache for that workflow.
   * Stable identity across reruns of the same workflow.
   */
  cid: string;
}

export interface StageResult<TOutput> {
  /** The stage's output — either freshly computed or reconstructed from cache. */
  output: TOutput;

  /**
   * The CID of the memento for this stage run. Pass this as part
   * of the next stage's inputCids to thread the DAG.
   */
  cid: string;

  /** Whether the runner skipped run() and reconstructed from cache. */
  cacheHit: boolean;
}
