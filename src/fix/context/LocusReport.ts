/**
 * The artifact produced by Locate (B2).
 *
 * Locate resolves a bug-report's code references (or Investigate's
 * primary location) to a concrete SAST node. The artifact carries:
 *   - The resolved BugLocus (file, line, function, primaryNode, ...)
 *   - The confidence Locate assigned (already on BugLocus as 0..1)
 *   - The rationale (which code-reference matched, by what mechanism)
 *
 * Downstream prompts cite this artifact so C3 and C5 see WHY Locate
 * picked this node, not just WHICH node. That informs calibration:
 * a single decisive match warrants stronger downstream commitment than
 * a noisy multi-candidate match.
 */

import type { BugLocus } from "../types.js";

export interface LocusReport {
  /** The resolved bug locus. Same shape Locate has always returned. */
  readonly locus: BugLocus;

  /**
   * How Locate found the node. One of:
   *   - "exact-match"     : code reference's file + function landed exactly
   *   - "suffix-match"    : code reference's file matched as a suffix of an indexed file
   *   - "function-only"   : function-name binding without a clean file match
   *   - "investigate-led" : Investigate's primaryLocation was the seed
   */
  readonly matchMechanism: "exact-match" | "suffix-match" | "function-only" | "investigate-led";

  /**
   * One-line rationale: what the code reference said and what Locate matched.
   * Downstream prompts cite verbatim.
   */
  readonly rationale: string;
}
