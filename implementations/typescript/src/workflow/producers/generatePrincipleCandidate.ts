/**
 * GeneratePrincipleCandidate stage — bug-fix workflow's principle minting (C6).
 *
 * Wraps generatePrincipleCandidate() in a Stage<I, O>. Returns
 * `PrincipleCandidate[]` (length 0..3): index 0 is the canonical shape;
 * remaining entries are alternative AST shapes of the same bug class
 * (all share `bugClassId`). Empty array means no principle was generated
 * (existing-principle-match, non-codifiable, or substrate failure).
 *
 * Cache contract — careful with side effects:
 *   The C6m mechanical-mode branch (when `recognized.matched === true`)
 *   appends a customer-fix provenance entry to the existing library
 *   JSON via `appendLibraryProvenance` — a disk-write side effect.
 *   On cache hit the provenance is NOT re-appended; consumers should
 *   read from the Stage's output (always `[]` in the recognized branch),
 *   not assume the file was just touched. This matches investigate.ts's
 *   "report file not re-created on cache hit" pattern.
 *
 *   The non-recognized branch may also issue LLM calls and substrate
 *   capability proposals, but those are pure — same input + same LLM
 *   → same array. Caching the array is legitimate.
 *
 * Input hashing notes:
 *   - signal, invariant, fixCandidate, recognized, projectRoot are
 *     all content-hashable. `recognized` is included verbatim because
 *     the matched/unmatched branch changes the output AND triggers
 *     different side effects.
 *   - overlay.baseRef is included (different starting code may change
 *     latentSiteMatches in the substrate path); overlay.worktreePath /
 *     sastDb are EXCLUDED (runtime resources).
 *
 * Construction-time deps: db, llm, optional logger. Per-call input
 * carries signal, invariant, fixCandidate, optional overlay,
 * recognized, projectRoot.
 */

import { generatePrincipleCandidate } from "../../fix/stages/generatePrincipleCandidate.js";
import type {
  BugSignal,
  FixCandidate,
  InvariantClaim,
  LLMProvider,
  OverlayHandle,
  PrincipleCandidate,
} from "../../fix/types.js";
import type { RecognizeResult } from "../../fix/stages/recognize.js";
import type { Db } from "../../db/index.js";
import type { FixLoopLogger } from "../../fix/logger.js";
import type { Stage } from "../types.js";

export const GENERATE_PRINCIPLE_CANDIDATE_CAPABILITY = "generate-principle-candidate";

export interface GeneratePrincipleCandidateStageInput {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  overlay?: OverlayHandle;
  recognized?: RecognizeResult;
  projectRoot?: string;
}

export interface MakeGeneratePrincipleCandidateStageDeps {
  db: Db;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Override producer identity. Default: "generatePrincipleCandidate@v1". */
  producerVersion?: string;
}

export function makeGeneratePrincipleCandidateStage(
  deps: MakeGeneratePrincipleCandidateStageDeps,
): Stage<GeneratePrincipleCandidateStageInput, PrincipleCandidate[]> {
  const producedBy = deps.producerVersion ?? "generatePrincipleCandidate@v1";

  return {
    name: "generatePrincipleCandidate",
    producedBy,

    serializeInput(input) {
      return {
        signal: input.signal,
        invariant: input.invariant,
        fixCandidate: input.fixCandidate,
        recognized: input.recognized ?? null,
        projectRoot: input.projectRoot ?? null,
        // Only the overlay's content-defining field participates.
        overlayBaseRef: input.overlay?.baseRef ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as PrincipleCandidate[];
    },

    async run(input) {
      return generatePrincipleCandidate({
        signal: input.signal,
        invariant: input.invariant,
        fixCandidate: input.fixCandidate,
        db: deps.db,
        llm: deps.llm,
        overlay: input.overlay,
        logger: deps.logger,
        recognized: input.recognized,
        projectRoot: input.projectRoot,
      });
    },
  };
}
