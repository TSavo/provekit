/**
 * The artifact produced by Investigate (B1.5).
 *
 * Investigate fires when Intake's BugReport doesn't carry usable code
 * references — typical for user-facing symptom-only bug reports. The
 * stage runs an LLM call with a project tour and asks for candidate
 * code sites. The result anchors every downstream stage:
 *   - Locate uses primaryLocation as its first-pass match target
 *   - C1 reads rootCauseHypothesis to write a strong invariant
 *   - C3 treats primaryLocation as a constraint, not a hint
 *   - C5 uses fixHypothesis to know what behavior the test should pin
 *
 * Re-exported from src/fix/stages/investigate.ts (where the stage logic
 * lives) so artifact types live alongside other context artifacts in
 * src/fix/context/.
 */

export type { InvestigateReport, CandidateLocation, ConfidenceTier }
  from "../stages/investigate.js";
