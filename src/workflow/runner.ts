/**
 * Workflow runner — the work-skipping engine.
 *
 * Spec: docs/specs/2026-04-29-workflows-as-first-class-primitive.md
 *
 * Single primitive: runStage(). Hash the input, look up the memento
 * store, return cached output on hit, run + persist on miss. The
 * caller threads CIDs forward — stage 2's inputCids = [stage1.cid].
 *
 * Work-skipping cascades because stages are deterministic: if stage 1
 * hits cache, its output is byte-identical to the original run, so
 * stage 2's serialized input is byte-identical, so stage 2 also hits,
 * and so on. A workflow whose every stage hits cache executes zero
 * producer work — pure DB reads.
 *
 * The runner stays workflow-agnostic. Specific workflows (bug-fix,
 * compliance-audit, property-assertion) are recipes that thread
 * runStage() calls. The runner doesn't know about any of them.
 */

import { findMemento, writeMemento, hashCanonical } from "../fix/runtime/mementoStore.js";
import type { Db } from "../db/index.js";
import type { Stage, StageResult, Workflow } from "./types.js";

export class WorkflowRunner {
  constructor(
    private readonly db: Db,
    private readonly workflow: Workflow,
  ) {}

  /**
   * Run a stage, threading its result through the memento store.
   *
   * - bindingHash = (workflow.cid, stage.name) — "where in the
   *   workflow we are."
   * - propertyHash = canonical-hash of stage.serializeInput(input)
   *   — "the work being done on this content."
   * - inputCids — upstream stage CIDs. The DAG edge that makes
   *   provenance walkable from any terminal stage.
   *
   * On cache hit: deserializeOutput(witness) reconstructs the output;
   * run() is never called. On miss: run produces output, witness
   * stores serializeOutput(output), the row gets inserted.
   */
  async runStage<TInput, TOutput>(
    stage: Stage<TInput, TOutput>,
    input: TInput,
    inputCids: string[] = [],
  ): Promise<StageResult<TOutput>> {
    const bindingHash = hashCanonical({
      workflow: this.workflow.cid,
      stage: stage.name,
    });
    const propertyHash = hashCanonical(stage.serializeInput(input));

    const existing = findMemento(this.db, { bindingHash, propertyHash });
    if (existing && existing.witness != null && existing.cid != null) {
      return {
        output: stage.deserializeOutput(existing.witness),
        cid: existing.cid,
        cacheHit: true,
      };
    }

    const output = await stage.run(input);
    const witness = stage.serializeOutput(output);
    const memento = writeMemento(this.db, {
      bindingHash,
      propertyHash,
      verdict: "holds",
      witness,
      producedBy: stage.producedBy,
      inputCids,
    });

    return {
      output,
      cid: memento.cid!,
      cacheHit: false,
    };
  }
}
