/**
 * provekit lift: synthesize a signed `.proof` from existing TypeScript.
 *
 * Public API:
 *   liftFile(filePath, options)   - run the highest-scoring adapter on
 *                                   one file. Throws LiftError on any
 *                                   diagnostic. Returns MintResult on
 *                                   success.
 *   registerAdapter(adapter)      - extension point for additional
 *                                   per-language adapters.
 *
 * The CLI surface (`provekit lift <file>.ts`) is wired in cli.ts in
 * run-2; v0 ships the library + scaffold only.
 *
 * Spec: docs/superpowers/specs/2026-04-30-provekit-lift-v0.md.
 */

import { tsPrimitiveAdapter, type LiftAdapter, type LiftInput } from "./tsPrimitiveAdapter.js";
import type { MintResult } from "./mint.js";
import { LiftError, makeDiagnostic } from "./errors.js";

export type { LiftAdapter, LiftInput };
export type { FunctionShape, FunctionParam, DetectResult } from "./detect.js";
export type { Candidate, ProposeResult, LiftLLM } from "./propose.js";
export type { FilterResult } from "./filter.js";
export type { Reviewer, Decision, ReviewResult } from "./review.js";
export type { MintResult, MintInput } from "./mint.js";
export {
  type LiftDiagnostic,
  type LiftDiagnosticCode,
  LiftError,
} from "./errors.js";

export { detect } from "./detect.js";
export { propose } from "./propose.js";
export { filter } from "./filter.js";
export { review } from "./review.js";
export { mint } from "./mint.js";
export { tsPrimitiveAdapter };

const adapters: LiftAdapter[] = [tsPrimitiveAdapter];

export function registerAdapter(adapter: LiftAdapter): void {
  adapters.push(adapter);
}

/** Test/inspection helper: enumerate registered adapters in declaration order. */
export function listAdapters(): readonly LiftAdapter[] {
  return adapters.slice();
}

const SCORE_THRESHOLD = 0.5;

export async function liftFile(
  filePath: string,
  input: Omit<LiftInput, "filePath"> = {},
): Promise<MintResult> {
  const ranked = adapters
    .map((a) => ({ adapter: a, score: a.detectScore(filePath) }))
    .filter((r) => r.score > 0)
    .sort((a, b) => b.score - a.score);

  if (ranked.length === 0 || ranked[0]!.score < SCORE_THRESHOLD) {
    throw new LiftError(
      makeDiagnostic(
        "unsupported-export-shape",
        filePath,
        0,
        `no lift adapter scored above ${SCORE_THRESHOLD} for ${filePath}; v0 supports .ts files with one exported function and primitive types only.`,
      ),
    );
  }

  const top = ranked[0]!.adapter;
  return top.liftToProof({ ...input, filePath });
}
