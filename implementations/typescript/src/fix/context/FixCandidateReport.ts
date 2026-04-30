/**
 * The artifact produced by C3 (generateFixCandidate).
 *
 * Wraps the existing FixCandidate type with calibration metadata:
 *   - Whether C3 honored the LocusReport's primary node (or overrode it)
 *   - If overridden, the structured locusDisagreement explaining why
 *
 * The override mechanism is the calibration channel for C3. Pre-refactor,
 * C3 silently patched a different file when it disagreed with Locate
 * (the 2026-04-27 promptlib dogfood: Locate said `repositories.ts`,
 * C3 patched `src/index.ts` because it preferred the consumer-layer
 * fix). With the override mechanism, disagreement is structured: C3
 * either patches at the locus or returns a `locusDisagreement` artifact
 * that downstream can act on (re-Locate, escalate, abort).
 */

import type { FixCandidate } from "../types.js";

export interface FixCandidateReport {
  /** The accepted patch + Z3 + audit data. */
  readonly candidate: FixCandidate;

  /**
   * Did C3 patch at the file LocusReport identified? When true, no
   * disagreement; when false, the LLM moved to a different layer and
   * we expect locusDisagreement to be populated for orchestrator review.
   */
  readonly honoredLocus: boolean;

  /**
   * Structured locus disagreement, set when honoredLocus is false.
   * Cite verbatim in the run's audit trail.
   */
  readonly locusDisagreement?: {
    readonly proposedFile: string;
    readonly proposedFunction?: string;
    readonly rationale: string;
  };
}
