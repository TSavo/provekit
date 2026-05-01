/**
 * Stage 4 of the prove-lift pipeline (STUB).
 *
 * Real Review presents each surviving candidate to the user via CLI:
 *   [a]ccept  [r]eject  [e]dit  [n]one  [q]uit
 *
 * v0 (this run) ships a stub that auto-accepts the first candidate so
 * downstream stages can be exercised without TTY interaction. The
 * stub provides a `reviewer` injection point so tests can supply a
 * deterministic decision function in lieu of real prompts.
 */

import type { FunctionShape } from "./detect.js";
import type { Candidate } from "./propose.js";
import type { LiftDiagnostic } from "./errors.js";

export type Decision =
  | { kind: "accept"; candidate: Candidate }
  | { kind: "reject"; candidate: Candidate }
  | { kind: "edit"; candidate: Candidate; replacement: Candidate }
  | { kind: "none" };

export interface Reviewer {
  decide(shape: FunctionShape, candidate: Candidate): Promise<Decision>;
}

export interface ReviewResult {
  accepted: Candidate[];
  diagnostics: LiftDiagnostic[];
}

export interface ReviewOptions {
  reviewer?: Reviewer;
}

export async function review(
  shape: FunctionShape,
  candidates: Candidate[],
  options: ReviewOptions = {},
): Promise<ReviewResult> {
  const reviewer = options.reviewer ?? autoAcceptFirstReviewer();
  const accepted: Candidate[] = [];
  for (const c of candidates) {
    const d = await reviewer.decide(shape, c);
    if (d.kind === "accept") {
      accepted.push(d.candidate);
      // v0 stops after one acceptance; multi-property output is run-2.
      return { accepted, diagnostics: [] };
    }
    if (d.kind === "edit") {
      accepted.push(d.replacement);
      return { accepted, diagnostics: [] };
    }
    if (d.kind === "none") {
      return { accepted, diagnostics: [] };
    }
    // reject -> continue
  }
  return { accepted, diagnostics: [] };
}

function autoAcceptFirstReviewer(): Reviewer {
  return {
    async decide(_shape, candidate) {
      return { kind: "accept", candidate };
    },
  };
}
