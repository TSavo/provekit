/**
 * Recognize stage — bug-fix workflow's principle-template matching (B3).
 *
 * Wraps recognize() in a Stage<I, O>. Pure SAST + DSL evaluation: no
 * LLM, no Z3. Reads the on-disk principle library, runs DSL queries
 * against the locus's SAST file, and returns the highest-confidence
 * matching principle whose root intersects the locus's primary node.
 *
 * Construction-time deps: db, optional logger, optional preloaded
 * principle library (runtime resource — NOT part of the hash). Per-call
 * input: locus.
 *
 * Cache contract / staleness:
 *   The principle library on disk is NOT hashed into the input. Same
 *   caveat formulate.ts documents: when the principle library evolves
 *   (a new principle is minted, an existing JSON is mutated), the
 *   cached recognize result silently goes stale. v1 ships with this
 *   limitation; the principle-library-hash binding is a follow-up.
 *
 * Output shape: RecognizeResult, a discriminated union of
 * `{ matched: false }` and the matched payload (principleId, bindings,
 * full LibraryPrinciple, matchId, rootMatchNodeId). Round-trips fully
 * through JSON — every field is primitive / array / nested primitive.
 */

import { recognize, type RecognizeResult, type PrincipleLibrary } from "../../fix/stages/recognize.js";
import type { BugLocus } from "../../fix/types.js";
import type { FixLoopLogger } from "../../fix/logger.js";
import type { Db } from "../../db/index.js";
import type { Stage } from "../types.js";

export const RECOGNIZE_CAPABILITY = "recognize";

export interface RecognizeStageInput {
  locus: BugLocus;
}

export interface MakeRecognizeStageDeps {
  db: Db;
  logger?: FixLoopLogger;
  /**
   * Optional preloaded principle library. Test-only injection point.
   * Production callers should leave undefined — recognize() loads from
   * `.provekit/principles/` on demand. Excluded from the input hash
   * (it's a runtime resource, like db).
   */
  library?: PrincipleLibrary;
  /** Override producer identity. Default: "recognize@v1". */
  producerVersion?: string;
}

export function makeRecognizeStage(
  deps: MakeRecognizeStageDeps,
): Stage<RecognizeStageInput, RecognizeResult> {
  const producedBy = deps.producerVersion ?? "recognize@v1";

  return {
    name: "recognize",
    producedBy,

    serializeInput(input) {
      return {
        locus: input.locus,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as RecognizeResult;
    },

    async run(input) {
      return recognize({
        db: deps.db,
        locus: input.locus,
        library: deps.library,
        logger: deps.logger,
      });
    },
  };
}
