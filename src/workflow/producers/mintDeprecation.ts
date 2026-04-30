/**
 * MintDeprecation stage — terminal claim for the retire workflow.
 *
 * Produces a memento attesting that an existing invariant has been
 * deprecated. The audit trail is the memento itself — the source-level
 * `must.skip(...)` marker (written separately by the writeInvariantFile
 * action) is for human readers; the durable record of the deprecation
 * is the chain of mementos this Stage emits.
 *
 * Verdict semantics:
 *   - `decayed`: the existing invariant is no longer being asserted.
 *     This is the standard verdict for retiring an invariant; consumers
 *     walking the proof DAG see the decayed memento and stop counting
 *     the prior `holds` verdicts as live evidence.
 *
 * Note on workflow shape: the Stage's OUTPUT carries the rendered
 * deprecation record; the runner wraps it in the workflow-level
 * memento via runWorkflow(), and the inputCids of that wrapper memento
 * include this Stage's CID. Downstream auditors walking from the
 * workflow memento land at this Stage's `verdict: decayed` claim
 * directly.
 *
 * Spec:
 *   docs/specs/2026-04-29-correctness-is-a-hash.md §"Change the
 *     invariant, the hash changes" — silent deprecation is impossible;
 *     the propertyHash diff surfaces the change.
 */

import type { Stage } from "../types.js";

export const MINT_DEPRECATION_CAPABILITY = "mint-deprecation";

export interface MintDeprecationStageInput {
  /**
   * propertyHash of the invariant being retired. Recorded in the
   * deprecation record so the workflow's terminal memento can be
   * traced back to the specific invariant it deprecates.
   */
  retiredPropertyHash: string;
  /** Property name (matches the `property("name", ...)` declaration). */
  propertyName: string;
  /** Reason for retiring. Free-form text. Required — no silent retires. */
  reason: string;
  /**
   * Optional path to the `.invariant.ts` file on disk. Recorded for
   * human-facing rendering only; not load-bearing for the chain.
   */
  filePath?: string;
}

export interface MintDeprecationOutput {
  retiredPropertyHash: string;
  propertyName: string;
  reason: string;
  filePath: string | null;
  /**
   * The verdict carried by this Stage's memento. Always "decayed" in
   * v1 — present in the output so downstream consumers don't need
   * a second DB read to render the verdict.
   */
  verdict: "decayed";
  /** Free-form explanation rendered for human consumption. */
  text: string;
}

export interface MakeMintDeprecationStageDeps {
  /** Override producer identity. Default: "mintDeprecation@v1". */
  producerVersion?: string;
}

export function makeMintDeprecationStage(
  deps: MakeMintDeprecationStageDeps = {},
): Stage<MintDeprecationStageInput, MintDeprecationOutput> {
  const producedBy = deps.producerVersion ?? "mintDeprecation@v1";

  return {
    name: "mintDeprecation",
    producedBy,

    serializeInput(input) {
      return {
        retiredPropertyHash: input.retiredPropertyHash,
        propertyName: input.propertyName,
        reason: input.reason,
        filePath: input.filePath ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as MintDeprecationOutput;
    },

    async run(input) {
      if (!input.reason || input.reason.trim().length === 0) {
        throw new Error(
          "mintDeprecation requires a non-empty reason; silent retires are not allowed",
        );
      }
      const text = renderText(input);
      return {
        retiredPropertyHash: input.retiredPropertyHash,
        propertyName: input.propertyName,
        reason: input.reason,
        filePath: input.filePath ?? null,
        verdict: "decayed",
        text,
      };
    },
  };
}

function renderText(input: MintDeprecationStageInput): string {
  const lines: string[] = [];
  lines.push(`Retired invariant: ${input.propertyName}`);
  lines.push(`  propertyHash: ${input.retiredPropertyHash}`);
  if (input.filePath) lines.push(`  file: ${input.filePath}`);
  lines.push(`  reason: ${input.reason}`);
  lines.push("  verdict: decayed");
  return lines.join("\n");
}
