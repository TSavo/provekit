/**
 * Intake stage — bug-fix workflow's first capability.
 *
 * Wraps the existing intake adapter system as a Stage<I, O>. The
 * adapter selection, prompt construction, and LLM call all live
 * inside parseBugSignal/detectAndParseBugSignal; this file is the
 * thin contract that makes intake addressable from a workflow
 * manifest as `capability: intake`.
 *
 * Factory pattern: makeIntakeStage(llm) returns a Stage. The LLM
 * provider is the dependency that can't be hashed and so isn't part
 * of the Stage input — it's injected at construction. Two intake
 * stages with different LLMs would register as different producers
 * (different `producedBy`); cross-validation would then surface
 * disagreements between them.
 */

import { detectAndParseBugSignal, parseBugSignal } from "../../fix/intake.js";
import type { IntentSignal, LLMProvider } from "../../fix/types.js";
import type { Stage } from "../types.js";

export const INTAKE_CAPABILITY = "intake";

export interface IntakeStageInput {
  /** Verbatim user-supplied text — bug report, change request, property assertion. */
  text: string;
  /**
   * Optional explicit adapter name (matches a registered intake adapter).
   * If omitted, the runtime auto-detects via adapter.detect() scoring.
   */
  source?: string;
  /** Optional adapter-specific context (e.g. SAST finding, failing test snippet). */
  context?: unknown;
}

/**
 * Build an intake Stage bound to a specific LLM provider. Register it
 * against the "intake" capability in a ProducerRegistry to make it
 * addressable from a workflow manifest.
 *
 * Producer identity defaults to "intake@v1". When multiple LLM
 * providers run concurrently (cross-validation), pass producerVersion
 * to disambiguate — e.g. "intake@v1+claude-opus-4-7".
 */
export function makeIntakeStage(
  llm: LLMProvider,
  producerVersion: string = "intake@v1",
): Stage<IntakeStageInput, IntentSignal> {
  return {
    name: "intake",
    producedBy: producerVersion,

    serializeInput(input) {
      // Canonicalize: omit undefined-valued keys so the property hash
      // is stable across callers that pass {text} vs {text, source: undefined}.
      return {
        text: input.text,
        source: input.source ?? null,
        context: input.context ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as IntentSignal;
    },

    async run(input) {
      if (input.source) {
        return parseBugSignal(
          { text: input.text, source: input.source, context: input.context },
          llm,
        );
      }
      return detectAndParseBugSignal(
        { text: input.text, context: input.context },
        llm,
      );
    },
  };
}
