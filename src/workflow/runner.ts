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
import type { ProducerRegistry } from "./registry.js";
import type { Stage, StageResult, Workflow } from "./types.js";

const WORKFLOW_RUN_STAGE_NAME = "__workflow_run__";

export class WorkflowRunner {
  constructor(
    private readonly db: Db,
    private readonly workflow: Workflow,
    private readonly registry?: ProducerRegistry,
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

  /**
   * Capability-dispatch shorthand. Looks up the capability in the
   * registry and runs the resolved Stage. Throws if no producer is
   * registered. Workflows that prefer `request("intake", input)`
   * over `runStage(intakeStage, input)` use this; behavior is
   * otherwise identical to runStage.
   */
  async request<TInput, TOutput>(
    capability: string,
    input: TInput,
    inputCids: string[] = [],
  ): Promise<StageResult<TOutput>> {
    if (!this.registry) {
      throw new Error(
        `WorkflowRunner has no registry; pass one to the constructor to use request()`,
      );
    }
    const stage = this.registry.resolve<TInput, TOutput>(capability);
    if (!stage) {
      throw new Error(`no producer registered for capability "${capability}"`);
    }
    return this.runStage(stage, input, inputCids);
  }

  /**
   * Run an entire workflow as a single memento. The workflow body
   * orchestrates stages internally (calling runStage / request); its
   * return value bundles the output with the terminal stage's CID.
   * On cache hit, the body is never invoked — output is reconstructed
   * from the witness column of the workflow-level memento.
   *
   * This is what lets work-skipping cascade asymptotically: a
   * workflow whose input has been seen before returns its previous
   * output without invoking ANY stage. The workflow-level cache hit
   * is one DB read; without it, the cascade still works but requires
   * one DB read per stage.
   *
   * The workflow-level memento's inputCids = [terminalCid] — walking
   * from the workflow CID reaches the terminal stage, then the rest
   * of the DAG follows from there.
   */
  async runWorkflow<TInput, TOutput>(
    input: TInput,
    body: (runner: WorkflowRunner) => Promise<{ output: TOutput; cid: string }>,
    options: {
      serializeInput?: (input: TInput) => unknown;
      serializeOutput?: (output: TOutput) => string;
      deserializeOutput?: (witness: string) => TOutput;
    } = {},
  ): Promise<StageResult<TOutput>> {
    const serializeInput = options.serializeInput ?? ((x: TInput) => x);
    const serializeOutput =
      options.serializeOutput ?? ((x: TOutput) => JSON.stringify(x));
    const deserializeOutput =
      options.deserializeOutput ?? ((w: string) => JSON.parse(w) as TOutput);

    const bindingHash = hashCanonical({
      workflow: this.workflow.cid,
      stage: WORKFLOW_RUN_STAGE_NAME,
    });
    const propertyHash = hashCanonical(serializeInput(input));

    const existing = findMemento(this.db, { bindingHash, propertyHash });
    if (existing && existing.witness != null && existing.cid != null) {
      return {
        output: deserializeOutput(existing.witness),
        cid: existing.cid,
        cacheHit: true,
      };
    }

    const { output, cid: terminalCid } = await body(this);
    const memento = writeMemento(this.db, {
      bindingHash,
      propertyHash,
      verdict: "holds",
      witness: serializeOutput(output),
      producedBy: `workflow:${this.workflow.name}@${this.workflow.cid}`,
      inputCids: [terminalCid],
    });

    return {
      output,
      cid: memento.cid!,
      cacheHit: false,
    };
  }
}
