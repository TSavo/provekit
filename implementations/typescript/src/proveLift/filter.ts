/**
 * Stage 3 of the prove-lift pipeline (STUB).
 *
 * Real Filter discovers tests via the project's test runner, replays
 * each test's argument values against every candidate's antecedent,
 * and drops any candidate that says a passing test's input is illegal.
 *
 * v0 (this run) ships a stub that passes every candidate through.
 * Run-2 wires this to vitest. The interface below is stable; only the
 * implementation changes.
 */

import type { FunctionShape } from "./detect.js";
import type { Candidate } from "./propose.js";
import type { LiftDiagnostic } from "./errors.js";

export interface FilterResult {
  survivors: Candidate[];
  /** Per-candidate notes. Survives := all entries with `dropped: false`. */
  notes: Array<{
    candidate: Candidate;
    dropped: boolean;
    reason?: string;
    testsExercised: number;
  }>;
  diagnostics: LiftDiagnostic[];
}

export interface FilterOptions {
  /** Pre-discovered concrete inputs for the function under analysis. */
  testInputs?: unknown[][];
  /** Project root for test discovery. Run-2 uses this. */
  projectRoot?: string;
}

export async function filter(
  shape: FunctionShape,
  candidates: Candidate[],
  options: FilterOptions = {},
): Promise<FilterResult> {
  // Stub: pass everything through. Real implementation in run-2.
  void shape;
  void options;
  return {
    survivors: [...candidates],
    notes: candidates.map((c) => ({
      candidate: c,
      dropped: false,
      reason: "filter is a stub in v0; all candidates pass through.",
      testsExercised: 0,
    })),
    diagnostics: [],
  };
}
