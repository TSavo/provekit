/**
 * Investigate stage — bug-fix workflow's project-tour + LLM hypothesis.
 *
 * Wraps investigate() in a Stage<I, O>. The underlying function writes
 * a JSON report to <projectRoot>/.provekit/contexts/ as a side effect.
 * On cache hit the file is NOT re-created — consumers should read the
 * `report` / `codeReferences` fields from the Stage's output, not
 * re-read the file from `reportPath`. The path is preserved in the
 * cached output for diagnostic / audit-trail purposes only.
 *
 * Construction-time deps: llm, optional logger. Per-call inputs:
 * signal, projectRoot, optional caps.
 *
 * The hash includes projectRoot, which makes investigate mementos
 * machine-specific — a cache miss across machines that have the same
 * project at different paths. Cross-machine reuse will need a
 * content-addressed project identifier (git SHA, SAST DB hash) on
 * the binding side; that's a follow-up when swarm distribution lands.
 */

import { investigate } from "../../fix/stages/investigate.js";
import type { InvestigateResult } from "../../fix/stages/investigate.js";
import type { IntentSignal, LLMProvider } from "../../fix/types.js";
import type { FixLoopLogger } from "../../fix/logger.js";
import type { Stage } from "../types.js";

export const INVESTIGATE_CAPABILITY = "investigate";

export interface InvestigateStageInput {
  signal: IntentSignal;
  projectRoot: string;
  reportDir?: string;
  maxTreeEntries?: number;
}

export interface MakeInvestigateStageDeps {
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Override producer identity. Default: "investigate@v1". */
  producerVersion?: string;
}

export function makeInvestigateStage(
  deps: MakeInvestigateStageDeps,
): Stage<InvestigateStageInput, InvestigateResult> {
  const producedBy = deps.producerVersion ?? "investigate@v1";

  return {
    name: "investigate",
    producedBy,

    serializeInput(input) {
      return {
        signal: input.signal,
        projectRoot: input.projectRoot,
        reportDir: input.reportDir ?? null,
        maxTreeEntries: input.maxTreeEntries ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as InvestigateResult;
    },

    async run(input) {
      return investigate({
        signal: input.signal,
        projectRoot: input.projectRoot,
        llm: deps.llm,
        logger: deps.logger,
        reportDir: input.reportDir,
        maxTreeEntries: input.maxTreeEntries,
      });
    },
  };
}
