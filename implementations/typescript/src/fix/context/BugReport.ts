/**
 * The artifact produced by Intake.
 *
 * Wraps the existing BugSignal so the rest of the codebase that already
 * consumes BugSignal keeps working, while giving Intake's output a
 * stable name in the context bag and a confidence indicator the
 * downstream stages can read for calibration.
 */

import type { BugSignal } from "../types.js";

export interface BugReport {
  /** The parsed bug signal — symptom, codeReferences, fixHint, etc. */
  readonly signal: BugSignal;

  /**
   * Intake's confidence that the signal cleanly maps to a single bug class.
   * "high" — signal includes a tight code reference + a clear failure mode.
   * "medium" — partial code references or moderate failure ambiguity.
   * "low" — symptom-only, no usable references; downstream needs Investigate.
   */
  readonly confidence: "high" | "medium" | "low";

  /**
   * One-line rationale for the confidence rating. Downstream prompts cite this
   * verbatim so the LLM at C1/C3/C5 can self-calibrate.
   */
  readonly confidenceRationale: string;
}
