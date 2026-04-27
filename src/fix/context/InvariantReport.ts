/**
 * The artifact produced by C1 (formulateInvariant).
 *
 * C1 reads BugReport + LocusReport + InvestigateReport and produces a
 * formal invariant the patch must satisfy. The artifact carries:
 *   - The InvariantClaim itself (formal expression, bindings, kind)
 *   - C1's scope assessment: how broadly the invariant quantifies
 *   - The root-cause clauses C1 explicitly aimed to rule out
 *
 * The scope field is the calibration knob for C3:
 *   - "local"      : narrow logical claim about one expression
 *   - "method"     : claim about a single function's behavior
 *   - "module"     : claim about a class or module's data flow
 *   - "cross-module" : claim spanning multiple files (data flow)
 *
 * A "local" invariant tells C3 the patch can be small. A "module"
 * invariant tells C3 the patch must touch the data layer, not the
 * caller. The narrow-invariant failure mode from 2026-04-27's
 * promptlib dogfood (consumer-side sort accepted by Z3 even though
 * the data layer was wrong) is exactly what richer scope information
 * prevents.
 */

import type { InvariantClaim } from "../types.js";

export interface InvariantReport {
  /** The formal invariant the patch must satisfy under Z3. */
  readonly invariant: InvariantClaim;

  /**
   * How broadly the invariant quantifies over program state. Calibration
   * input for C3 (where to patch) and C6 (how broadly to write the
   * principle).
   */
  readonly scope: "local" | "method" | "module" | "cross-module";

  /**
   * Which clauses of Investigate's rootCauseHypothesis (or the bug
   * report's failureDescription) this invariant directly rules out.
   * Each entry is a quoted clause + a one-line note on which formal
   * sub-clause closes it. If an investigate-flagged root cause is NOT
   * covered, the invariant is too narrow and C1 should be re-prompted.
   */
  readonly addressesRootCauseClauses: ReadonlyArray<{
    readonly clause: string;
    readonly closedBy: string;
  }>;

  /** C1's confidence the invariant is correct AND strong enough. */
  readonly confidence: "high" | "medium" | "low";

  /** Brief rationale; cited verbatim downstream. */
  readonly rationale: string;
}
