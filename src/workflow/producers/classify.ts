/**
 * Classify stage — bug-fix workflow's remediation-layer classification.
 *
 * Wraps classify() in a Stage<I, O>. Pure (LLM call + registry read);
 * no fs writes, no db writes. Same factory pattern as intake / formulate.
 *
 * Construction-time deps: llm, optional projectRoot for prompt-store
 * resolution. Per-call inputs: signal, locus (nullable). Output:
 * RemediationPlan.
 */

import { classify } from "../../fix/classify.js";
import type {
  BugLocus,
  IntentSignal,
  LLMProvider,
  RemediationPlan,
} from "../../fix/types.js";
import type { Stage } from "../types.js";

export const CLASSIFY_CAPABILITY = "classify";

export interface ClassifyStageInput {
  signal: IntentSignal;
  locus: BugLocus | null;
}

export interface MakeClassifyStageDeps {
  llm: LLMProvider;
  /**
   * Optional host-project root. When provided, the classify prompt
   * fragment resolves via better-prompts (per-project evolution).
   * Identical content either way at day 0.
   */
  projectRoot?: string;
  /** Override producer identity. Default: "classify@v1". */
  producerVersion?: string;
}

export function makeClassifyStage(
  deps: MakeClassifyStageDeps,
): Stage<ClassifyStageInput, RemediationPlan> {
  const producedBy = deps.producerVersion ?? "classify@v1";

  return {
    name: "classify",
    producedBy,

    serializeInput(input) {
      return {
        signal: input.signal,
        locus: input.locus ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as RemediationPlan;
    },

    async run(input) {
      return classify(input.signal, input.locus, deps.llm, deps.projectRoot);
    },
  };
}
