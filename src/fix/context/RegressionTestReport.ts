/**
 * The artifact produced by C5 (generateRegressionTest).
 *
 * C5 reads BugReport + InvestigateReport + LocusReport + InvariantReport
 * + FixCandidateReport and writes a regression test that:
 *   - FAILS against the original buggy code
 *   - PASSES against the fixed code
 *   - Reproduces the symptom AT THE SCALE the bug report describes
 *
 * The "scale" piece is the calibration knob C5 was missing in the
 * 2026-04-27 promptlib dogfood: the bug fired only at >25 invocations,
 * so a happy-path 3-element fixture would have falsely passed against
 * the buggy code. Tests must construct the actual reproduction scale.
 */

import type { TestArtifact } from "../types.js";

export interface RegressionTestReport {
  /** The generated test file + metadata + oracle #9 results. */
  readonly test: TestArtifact;

  /**
   * What scale of reproduction the test constructs (e.g., "27 invocations
   * with 7 fail signals on the most recent 7"). Cited verbatim in audits
   * so reviewers can see if the test stress-shape matches the bug-report
   * threshold.
   */
  readonly reproductionScale: string;

  /** Did the test actually fail against the buggy code under oracle #9b? */
  readonly failsOnBuggyCode: boolean;
  /** Did the test actually pass against the fixed code under oracle #9a? */
  readonly passesOnFixedCode: boolean;

  /** C5's confidence the test is a faithful reproduction. */
  readonly confidence: "high" | "medium" | "low";
}
