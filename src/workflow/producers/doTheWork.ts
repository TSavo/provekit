/**
 * Do-the-work stage — bug-fix workflow's unified patch+test generation.
 *
 * Wraps doTheWork() in a Stage<I, O>. This is the stage that does the
 * heavy lifting: an LLM agent mutates the overlay worktree, captures
 * the diff as a CodePatch, and the result includes the Oracle #2
 * (invariantHoldsUnderOverlay) and Oracle #9 (test failsOnOriginalCode +
 * passesOnFixedCode) verdicts.
 *
 * Cache contract (the important one):
 *   The memento IS the unit of work. A do-the-work memento records:
 *     - the patch (CodePatch)
 *     - the regression test (TestArtifact)
 *     - Oracle #2's verdict on this patch under this overlay
 *     - Oracle #9's verdict on this test under this patch
 *   On cache hit, ALL of these are reconstructed from the witness.
 *   Downstream stages read the verdict fields from the result rather
 *   than re-running verification — the cache hit IS the proof.
 *
 *   The overlay is NOT re-mutated on cache hit. The cached patch text
 *   is the description of "what would happen"; bundling consumes the
 *   patch + verdicts, not the live overlay state. If a future consumer
 *   needs the overlay in the patched state, it can apply the cached
 *   patch as a separate mechanical step (no LLM, no Z3).
 *
 * Input hashing notes:
 *   - signal, locus, invariant, investigateReport are content-hashable
 *     and included in the property hash.
 *   - overlay.baseRef is included (different starting code = different
 *     patch).
 *   - overlay.worktreePath, overlay.sastDb are EXCLUDED — they're
 *     per-run runtime resources, not content. Two overlays from the
 *     same baseRef should hit the same cache.
 *   - testRunner is excluded as a test-only injection.
 *
 * Construction-time deps: llm, optional logger + projectRoot. Per-call
 * input includes the overlay (whose runtime fields aren't hashed but
 * are passed through to doTheWork()).
 */

import { doTheWork } from "../../fix/stages/doTheWork.js";
import type { DoTheWorkResult } from "../../fix/stages/doTheWork.js";
import type {
  BugLocus,
  IntentSignal,
  InvariantClaim,
  LLMProvider,
  OverlayHandle,
} from "../../fix/types.js";
import type { InvestigateReport } from "../../fix/stages/investigate.js";
import type { FixLoopLogger } from "../../fix/logger.js";
import type { Stage } from "../types.js";

export const DO_THE_WORK_CAPABILITY = "do-the-work";

export interface DoTheWorkStageInput {
  signal: IntentSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  investigateReport?: InvestigateReport;
  /** Test-only injection. Hash-excluded. */
  testRunner?: (
    overlay: OverlayHandle,
    testFilePath: string,
    mainRepoRoot: string,
  ) => { exitCode: number; stdout: string; stderr: string };
}

export interface MakeDoTheWorkStageDeps {
  llm: LLMProvider;
  logger?: FixLoopLogger;
  projectRoot?: string;
  /** Override producer identity. Default: "do-the-work@v1". */
  producerVersion?: string;
}

export function makeDoTheWorkStage(
  deps: MakeDoTheWorkStageDeps,
): Stage<DoTheWorkStageInput, DoTheWorkResult> {
  const producedBy = deps.producerVersion ?? "do-the-work@v1";

  return {
    name: "do-the-work",
    producedBy,

    serializeInput(input) {
      return {
        signal: input.signal,
        locus: input.locus,
        invariant: input.invariant,
        investigateReport: input.investigateReport ?? null,
        // Only the overlay's content-defining fields participate in
        // the hash. Runtime fields (worktreePath, sastDb, modifiedFiles,
        // closed) are excluded — same baseRef = same starting code.
        overlayBaseRef: input.overlay.baseRef,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as DoTheWorkResult;
    },

    async run(input) {
      return doTheWork({
        signal: input.signal,
        locus: input.locus,
        invariant: input.invariant,
        overlay: input.overlay,
        llm: deps.llm,
        investigateReport: input.investigateReport,
        projectRoot: deps.projectRoot,
        logger: deps.logger,
        testRunner: input.testRunner,
      });
    },
  };
}
