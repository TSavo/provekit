/**
 * The artifact produced by C6 (generatePrincipleCandidate).
 *
 * C6 distills the bug fix into 1-3 alternative DSL principle shapes
 * for the library so the same shape gets caught at lint time on any
 * future code path. The artifact carries:
 *   - The principle candidates (DSL source + fixTemplate + testTemplate)
 *   - C6's confidence per candidate
 *   - The scope C6 generalized to: identical to InvariantReport.scope
 *     unless C6 narrowed (rare) or broadened (with explicit rationale)
 */

import type { PrincipleCandidate } from "../types.js";

export interface PrincipleReport {
  /** Principle candidates ranked by C6's confidence descending. */
  readonly candidates: ReadonlyArray<PrincipleCandidate>;

  /**
   * For each candidate (same order), the scope C6 generalized the
   * principle to. Local principles catch only the exact shape; broader
   * principles catch entire bug classes.
   */
  readonly perCandidateScope: ReadonlyArray<"local" | "method" | "module" | "cross-module">;

  /**
   * Why C6 chose to broaden (or narrow) from the InvariantReport's
   * scope. If empty array means C6 stayed at the invariant's scope.
   */
  readonly scopeAdjustmentRationales: ReadonlyArray<string>;

  /** Per-candidate confidence: high = ready to commit; lower = staged for review. */
  readonly perCandidateConfidence: ReadonlyArray<"high" | "medium" | "low">;
}
