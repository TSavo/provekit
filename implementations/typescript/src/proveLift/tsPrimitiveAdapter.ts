/**
 * The v0 lift adapter: TypeScript files with primitive arg/return types.
 *
 * Adapters are score-then-load: detectScore() returns 0..1; the
 * dispatcher picks the highest-scoring adapter and invokes liftToProof.
 * Today the registry has only this one adapter; the shape leaves room
 * for Rust / Go / Python adapters later without restructuring the CLI.
 *
 * Spec: docs/superpowers/specs/2026-04-30-provekit-lift-v0.md.
 */

import { existsSync, readFileSync } from "node:fs";

import { detect } from "./detect.js";
import { propose, type LiftLLM } from "./propose.js";
import { filter } from "./filter.js";
import { review, type Reviewer } from "./review.js";
import { mint, type MintResult } from "./mint.js";
import { LiftError, makeDiagnostic } from "./errors.js";

export interface LiftInput {
  filePath: string;
  llm?: LiftLLM;
  reviewer?: Reviewer;
  outPath?: string;
  privateKeyPem?: string;
}

export interface LiftAdapter {
  /** Stable adapter id (used in diagnostics + producer attribution). */
  id: string;
  /**
   * Score 0..1 of how well this adapter handles a given file.
   * 0 means refusal; the dispatcher will not load this adapter for
   * this input.
   */
  detectScore(filePath: string): number;
  /** Run the full five-stage pipeline and return the .proof location. */
  liftToProof(input: LiftInput): Promise<MintResult>;
}

export const tsPrimitiveAdapter: LiftAdapter = {
  id: "ts-primitive@0",

  detectScore(filePath: string): number {
    if (!filePath.endsWith(".ts") && !filePath.endsWith(".tsx")) return 0;
    if (filePath.endsWith(".invariant.ts")) return 0;
    if (filePath.endsWith(".test.ts")) return 0;
    if (filePath.endsWith(".d.ts")) return 0;
    if (!existsSync(filePath)) return 0;
    // Quick signal: file has at least one `export function` or
    // `export const ... =`. Real check happens in detect().
    let head: string;
    try {
      head = readFileSync(filePath, "utf8");
    } catch {
      return 0;
    }
    if (/\bexport\s+(function|const)\s+\w+/.test(head)) return 1;
    return 0;
  },

  async liftToProof(input: LiftInput): Promise<MintResult> {
    // Stage 1: Detect (real).
    const detectResult = detect(input.filePath);
    const shape = detectResult.shape;

    // Stage 2: Propose (stub today; LLM-driven run-2).
    const proposeResult = await propose(shape, input.llm ? { llm: input.llm } : {});
    if (proposeResult.candidates.length === 0) {
      throw new LiftError(
        makeDiagnostic(
          "all-candidates-dropped",
          shape.filePath,
          0,
          `no candidates produced by Propose for ${shape.name}`,
        ),
      );
    }

    // Stage 3: Filter (stub today; vitest-driven run-2).
    const filterResult = await filter(shape, proposeResult.candidates);
    if (filterResult.survivors.length === 0) {
      throw new LiftError(
        makeDiagnostic(
          "all-candidates-dropped",
          shape.filePath,
          0,
          `every candidate for ${shape.name} was rejected by the test oracle`,
        ),
      );
    }

    // Stage 4: Review (stub today; auto-accept first).
    const reviewResult = await review(
      shape,
      filterResult.survivors,
      input.reviewer ? { reviewer: input.reviewer } : {},
    );
    if (reviewResult.accepted.length === 0) {
      throw new LiftError(
        makeDiagnostic(
          "all-candidates-dropped",
          shape.filePath,
          0,
          `reviewer accepted no candidates for ${shape.name}`,
        ),
      );
    }

    // Stage 5: Mint (stub today; throws).
    const mintInput = {
      shape,
      accepted: reviewResult.accepted,
      ...(input.outPath !== undefined ? { outPath: input.outPath } : {}),
      ...(input.privateKeyPem !== undefined
        ? { privateKeyPem: input.privateKeyPem }
        : {}),
    };
    return mint(mintInput);
  },
};
