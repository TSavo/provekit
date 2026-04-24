/**
 * B5: Fix-loop orchestrator.
 *
 * Wires C1 → C2 → C3 → C4 → C5 → C6 → D1 → D2 → D3.
 * Every downstream stage is currently a stub that throws NotImplementedError.
 * The orchestrator catches NotImplementedError and converts it to a graceful
 * abort entry in the audit trail — distinct from a real runtime error.
 */

import type {
  BugSignal,
  BugLocus,
  RemediationPlan,
  LLMProvider,
  FixLoopResult,
  FixBundle,
  AuditEntry,
  OverlayHandle,
} from "./types.js";
import { NotImplementedError } from "./types.js";
import type { Db } from "../db/index.js";
import { formulateInvariant } from "./stages/formulateInvariant.js";
import { openOverlay } from "./stages/openOverlay.js";
import { generateFixCandidate } from "./stages/generateFixCandidate.js";
import { generateComplementary } from "./stages/generateComplementary.js";
import { generateRegressionTest } from "./stages/generateRegressionTest.js";
import { generatePrincipleCandidate } from "./stages/generatePrincipleCandidate.js";
import { assembleBundle } from "./stages/assembleBundle.js";
import { applyBundle } from "./stages/applyBundle.js";
import { learnFromBundle } from "./stages/learnFromBundle.js";

export interface RunFixLoopArgs {
  signal: BugSignal;
  locus: BugLocus;
  plan: RemediationPlan;
  db: Db;
  llm: LLMProvider;
  options: {
    autoApply: boolean;
    /** Cap on complementary sites discovered. Default 10. */
    maxComplementarySites: number;
    /** Minimum confidence to proceed through bundle assembly. Default 0.8. */
    confidenceThreshold: number;
  };
  /**
   * Injectable vitest runner for D1b oracle #10.
   * When provided, replaces real full-suite vitest in assembleBundle.
   * Signature: (overlay) → { exitCode, stdout, stderr }.
   */
  vitestRunner?: (overlay: OverlayHandle) => { exitCode: number; stdout: string; stderr: string };
  /**
   * Injectable test runner for C5 (oracle #9).
   * When provided, replaces real vitest-in-overlay for single-test regression runs.
   * Signature: (overlay, testFilePath, mainRepoRoot) → { exitCode, stdout, stderr }.
   * Called twice per C5 run: once against fixed code (expect 0), once against original (expect non-0).
   */
  c5TestRunner?: (overlay: OverlayHandle, testFilePath: string, mainRepoRoot: string) => { exitCode: number; stdout: string; stderr: string };
}

export async function runFixLoop(args: RunFixLoopArgs): Promise<FixLoopResult> {
  const audit: AuditEntry[] = [];

  try {
    // Stage C1: formulate invariant
    const invariant = await runStage("C1", "formulateInvariant", audit, () =>
      formulateInvariant({ signal: args.signal, locus: args.locus, db: args.db, llm: args.llm }),
    );

    // Stage C2: open overlay worktree + reindex
    const overlay = await runStage("C2", "openOverlay", audit, () =>
      openOverlay({ locus: args.locus, db: args.db }),
    );

    // Stage C3: generate fix candidate
    const fix = await runStage("C3", "generateFixCandidate", audit, () =>
      generateFixCandidate({ signal: args.signal, locus: args.locus, invariant, overlay, llm: args.llm }),
    );

    // Stage C4: generate complementary changes
    const complementary = await runStage("C4", "generateComplementary", audit, () =>
      generateComplementary({
        fix,
        locus: args.locus,
        overlay,
        db: args.db,
        llm: args.llm,
        maxSites: args.options.maxComplementarySites,
      }),
    );

    // Stage C5: generate regression test
    const test = await runStage("C5", "generateRegressionTest", audit, () =>
      generateRegressionTest({ fix, signal: args.signal, locus: args.locus, overlay, invariant, llm: args.llm, testRunner: args.c5TestRunner }),
    );

    // Stage C6: generate principle candidate (may be plain or with capability spec)
    const principle = await runStage("C6", "generatePrincipleCandidate", audit, () =>
      generatePrincipleCandidate({ signal: args.signal, invariant, fixCandidate: fix, db: args.db, llm: args.llm, overlay }),
    );

    // Stage D1: assemble bundle
    const bundle = await runStage("D1", "assembleBundle", audit, () =>
      assembleBundle({
        signal: args.signal,
        plan: args.plan,
        locus: args.locus,
        fix,
        complementary,
        test,
        principle,
        overlay,
        db: args.db,
        existingAuditTrail: audit,
        vitestRunner: args.vitestRunner,
      }),
    );

    // Stage D2: apply bundle
    const applyResult = await runStage("D2", "applyBundle", audit, () =>
      applyBundle({ bundle, options: { autoApply: args.options.autoApply, prDraftMode: !args.options.autoApply }, db: args.db }),
    );

    // Stage D3: learn from bundle (only if apply succeeded)
    if (applyResult.applied) {
      await runStage("D3", "learnFromBundle", audit, () =>
        learnFromBundle({ bundle, applyResult, db: args.db }),
      );
    }

    return {
      bundle,
      applied: applyResult.applied,
      auditTrail: audit,
      reason: applyResult.applied ? undefined : applyResult.failureReason,
      applyResult,
    };
  } catch (err) {
    if (err instanceof NotImplementedError) {
      // Graceful abort: record a skipped entry so tests can assert on the kind.
      audit.push({
        stage: err.stageId,
        kind: "skipped",
        detail: err.message,
        timestamp: Date.now(),
      });
      return {
        bundle: null,
        applied: false,
        auditTrail: audit,
        reason: `aborted at stage ${err.stageId}: ${err.message}`,
      };
    }
    // Unexpected error: record under "orchestrator" and return.
    audit.push({
      stage: "orchestrator",
      kind: "error",
      detail: err instanceof Error ? err.message : String(err),
      timestamp: Date.now(),
    });
    return {
      bundle: null,
      applied: false,
      auditTrail: audit,
      reason: err instanceof Error ? err.message : String(err),
    };
  }
}

/**
 * Run a single stage:
 *  1. Push a "start" entry.
 *  2. Await the stage function.
 *  3. On success: push a "complete" entry and return the result.
 *  4. On NotImplementedError: rethrow (outer handler records "skipped").
 *  5. On any other error: push an "error" entry under the stage, then rethrow.
 */
async function runStage<T>(
  stageId: string,
  stageName: string,
  audit: AuditEntry[],
  fn: () => Promise<T>,
): Promise<T> {
  audit.push({ stage: stageId, kind: "start", detail: stageName, timestamp: Date.now() });
  try {
    const result = await fn();
    audit.push({ stage: stageId, kind: "complete", detail: stageName, timestamp: Date.now() });
    return result;
  } catch (err) {
    if (err instanceof NotImplementedError) {
      // Let outer catch handle — don't record here, outer push the "skipped" entry.
      throw err;
    }
    // Real error: record it under this stage before rethrowing to outer catch.
    audit.push({
      stage: stageId,
      kind: "error",
      detail: err instanceof Error ? err.message : String(err),
      timestamp: Date.now(),
    });
    throw err;
  }
}

// Re-export FixBundle so callers don't have to reach into types.ts directly.
export type { FixBundle };
