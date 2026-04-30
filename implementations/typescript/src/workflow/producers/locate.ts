/**
 * Locate stage — bug-fix workflow's bug-locus resolution.
 *
 * Wraps locate() in a Stage<I, O>. Synchronous in the underlying
 * impl (DB queries only — no LLM, no Z3, no fs); the Stage's
 * run() returns the result inside a resolved promise to fit the
 * async contract. Same factory pattern as the others.
 *
 * Construction-time deps: db. Per-call inputs: signal. Output:
 * BugLocus | null (null when no candidate node ranks).
 */

import { locate } from "../../fix/locate.js";
import type { BugLocus, IntentSignal } from "../../fix/types.js";
import type { Db } from "../../db/index.js";
import type { Stage } from "../types.js";

export const LOCATE_CAPABILITY = "locate";

export interface LocateStageInput {
  signal: IntentSignal;
}

export interface MakeLocateStageDeps {
  db: Db;
  /** Override producer identity. Default: "locate@v1". */
  producerVersion?: string;
}

export function makeLocateStage(
  deps: MakeLocateStageDeps,
): Stage<LocateStageInput, BugLocus | null> {
  const producedBy = deps.producerVersion ?? "locate@v1";

  return {
    name: "locate",
    producedBy,

    serializeInput(input) {
      return { signal: input.signal };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as BugLocus | null;
    },

    async run(input) {
      return locate(deps.db, input.signal);
    },
  };
}
