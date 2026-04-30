/**
 * GenerateComplementary stage — bug-fix workflow's adjacent-site fixes (C4).
 *
 * Wraps generateComplementary() in a Stage<I, O>. Same shape as
 * doTheWork: the memento captures the unit of work in full (each
 * accepted ComplementaryChange already carries its Oracle #3 verdict
 * as `verifiedAgainstOverlay` and `overlayZ3Verdict`).
 *
 * Cache contract — verdict-bearing memento:
 *   The memento IS the unit of work. A generateComplementary memento
 *   records the array of accepted ComplementaryChanges; each entry
 *   already includes its overlay-verified verdict. On cache hit the
 *   downstream consumer trusts those verdicts without re-running
 *   verification — the cache hit IS the proof.
 *
 *   Determinism note: generateComplementary's loop walks discovered
 *   sites cumulatively, applying accepted patches into the overlay and
 *   rolling rejected ones back via verifySiteChange. Same baseRef +
 *   same fix + same locus + same invariant produces the same final
 *   accepted set, because each verifySiteChange rolls rejected patches
 *   back to a clean state before the next site is tried. Reproducing
 *   the same result from a fresh overlay is the deterministic claim
 *   that makes caching legitimate here.
 *
 *   The overlay is NOT re-mutated on cache hit. Cached patches describe
 *   "what would happen"; the bundling stage consumes the patch + verdict
 *   pair, not the live overlay state.
 *
 * Input hashing notes:
 *   - signal, locus, fix, invariant, maxSites are content-hashable.
 *   - overlay.baseRef is included; overlay.worktreePath/sastDb are
 *     EXCLUDED (runtime resources, same as doTheWork).
 *
 * Construction-time deps: db, llm, optional logger. Per-call input
 * carries the overlay (whose runtime fields aren't hashed but are
 * passed through to generateComplementary()).
 */

import { generateComplementary } from "../../fix/stages/generateComplementary.js";
import type {
  BugLocus,
  ComplementaryChange,
  FixCandidate,
  InvariantClaim,
  LLMProvider,
  OverlayHandle,
} from "../../fix/types.js";
import type { Db } from "../../db/index.js";
import type { FixLoopLogger } from "../../fix/logger.js";
import type { Stage } from "../types.js";

export const GENERATE_COMPLEMENTARY_CAPABILITY = "generate-complementary";

export interface GenerateComplementaryStageInput {
  fix: FixCandidate;
  locus: BugLocus;
  overlay: OverlayHandle;
  maxSites: number;
  invariant?: InvariantClaim;
}

export interface MakeGenerateComplementaryStageDeps {
  db: Db;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Override producer identity. Default: "generateComplementary@v1". */
  producerVersion?: string;
}

export function makeGenerateComplementaryStage(
  deps: MakeGenerateComplementaryStageDeps,
): Stage<GenerateComplementaryStageInput, ComplementaryChange[]> {
  const producedBy = deps.producerVersion ?? "generateComplementary@v1";

  return {
    name: "generateComplementary",
    producedBy,

    serializeInput(input) {
      return {
        fix: input.fix,
        locus: input.locus,
        maxSites: input.maxSites,
        invariant: input.invariant ?? null,
        // Only the overlay's content-defining field participates.
        overlayBaseRef: input.overlay.baseRef,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ComplementaryChange[];
    },

    async run(input) {
      return generateComplementary({
        fix: input.fix,
        locus: input.locus,
        overlay: input.overlay,
        db: deps.db,
        llm: deps.llm,
        maxSites: input.maxSites,
        invariant: input.invariant,
        logger: deps.logger,
      });
    },
  };
}
