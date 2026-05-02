// @ts-nocheck
/**
 * Bug-fix smoke fixture: a bounded parseInt-style bug.
 *
 * `parseByte` should parse a string into an integer clamped to [0, 255].
 * The ACTUAL implementation has a bug: it truncates but never clamps.
 *
 * The Zod schema below documents the CORRECT contract. The bug-fix
 * workflow lifts this contract, mints it as a signed memento, and
 * bundles it into a deterministic .proof -- so the fix evidence is
 * content-addressed and verifiable across kit boundaries.
 *
 * This fixture is a LIFT TARGET. The lifter walks the AST; the code is
 * never executed at test time. @ts-nocheck + @ts-ignore keep the file
 * parseable without zod/runtime deps installed.
 */

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore -- zod is consumed AST-only by the lifter.
import { z } from "zod";

/**
 * Buggy implementation: parses an integer string but never clamps to
 * the advertised [0, 255] range.
 */
export function parseByte(s: string): number {
  const n = Number(s);
  if (Number.isNaN(n)) return NaN;
  // BUG: truncates but does NOT clamp to [0, 255].
  return Math.trunc(n);
}

/**
 * The contract the fix commits to: any valid byte parse must produce an
 * integer in [0, 255]. The lifter lifts this into a forall-IR formula
 * that the verifier can discharge against the implementation.
 */
export const ByteSchema = z.number().int().min(0).max(255);
