/**
 * The artifact produced by Classify (B5).
 *
 * Classify reads BugReport + LocusReport and emits a RemediationPlan:
 * which fix-loop layer (code patch vs substrate extension), which model
 * tier per stage, which oracles will run. Wrapped here as a context
 * artifact so downstream stages can read the plan + Classify's reasoning.
 */

import type { RemediationPlan } from "../types.js";

export interface ClassifyReport {
  /** The remediation plan: layer, model tiers, oracle list. */
  readonly plan: RemediationPlan;

  /**
   * Classify's confidence in the plan choice. Downstream stages that
   * disagree (e.g., C3 wanting to escalate to substrate-extension when
   * Classify said code-patch) can cite this for an override.
   */
  readonly confidence: "high" | "medium" | "low";

  /** Why this layer and these tiers were chosen. Cited in downstream prompts. */
  readonly rationale: string;
}
