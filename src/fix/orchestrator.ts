/**
 * B5: Fix-loop orchestrator.
 *
 * Wires C1 → C2 → C3 → C4 → C5 → C6 → D1 → D2 → D3.
 * Every downstream stage is currently a stub that throws NotImplementedError.
 * The orchestrator catches NotImplementedError and converts it to a graceful
 * abort entry in the audit trail — distinct from a real runtime error.
 *
 * Architecture v2 (2026-04-27 spec): the orchestrator is an
 * **artifact-stream collector**. Each stage produces 0 or 1 artifacts; the
 * orchestrator collects them as they are produced and flushes them in a
 * dedicated `finally` step at the end. This matters because some artifacts
 * (notably the StoredInvariant) MUST be persisted even when downstream
 * stages fail — the invariant is the durable proof of the constraint, the
 * fix bundle is one consumer of it. Cross-stage flush gating is removed.
 *
 * Cross-stage DATA flow is NOT removed. C3 still consumes C1's invariant;
 * D1 still consumes C3's fix. What is removed is "downstream failure blocks
 * upstream artifact persistence."
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
  InvariantClaim,
  TestArtifact,
  FixCandidate,
} from "./types.js";
import type { InvestigateReport } from "./stages/investigate.js";
import { NotImplementedError } from "./types.js";
import { createNoopLogger, type FixLoopLogger } from "./logger.js";
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
import { recognize, type RecognizeResult } from "./stages/recognize.js";
import { buildStoredInvariant, writeInvariant } from "./runtime/invariantStore.js";
import {
  pickPrimaryPatchEdit,
  findExpressionLines,
  findFunctionLine,
} from "./runtime/patchUtils.js";
import type { Artifact, InvariantArtifact } from "../integration/interfaces.js";
import { execFileSync } from "child_process";
import { dirname } from "path";

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
  /** Logger for stage entry/exit markers and LLM call metrics. Defaults to noop. */
  logger?: FixLoopLogger;
  /**
   * Investigate's full report when the symptom-only path was used. Carries
   * primaryLocation, candidateLocations, rootCauseHypothesis, fixHypothesis.
   * Downstream stages (C1, C3, C5) cite these in their reasoner prompts so
   * each LLM sees the same upstream evidence and can self-calibrate.
   * Undefined when Intake produced clean code references and Investigate
   * didn't fire.
   */
  investigateReport?: InvestigateReport;
}

/**
 * Result extension that exposes the artifact stream collected during the
 * run. Existing fields on FixLoopResult are preserved; integrators wanting
 * the unfiltered artifact stream consume `artifacts`. We declare it as a
 * type *alias* with `artifacts` optional so existing mocks producing a
 * plain FixLoopResult remain assignable.
 */
export type FixLoopResultWithArtifacts = FixLoopResult & {
  /**
   * Every artifact emitted during the run, in order of emission. Includes
   * artifacts that were already persisted (their flush ran successfully)
   * AND artifacts that survived a downstream failure. Inspecting this
   * array is the canonical way to see "what got produced." Optional for
   * backward compatibility with mocks that returned FixLoopResult.
   */
  artifacts?: Artifact[];
};

export async function runFixLoop(args: RunFixLoopArgs): Promise<FixLoopResultWithArtifacts> {
  const audit: AuditEntry[] = [];
  const logger = args.logger ?? createNoopLogger();
  const artifacts: Artifact[] = [];

  // Capture variables that are filled in along the way. We keep them at
  // function scope so the `finally` block can flush them regardless of
  // where the main flow aborted.
  let invariant: InvariantClaim | null = null;
  let fix: FixCandidate | null = null;
  let test: TestArtifact | null = null;
  let invariantPersisted = false;

  let result: FixLoopResultWithArtifacts;

  try {
    // -----------------------------------------------------------------------
    // Stage B3: Recognize.
    // -----------------------------------------------------------------------
    logger.stage("B3: recognize");
    const t0b3 = Date.now();
    const recognized: RecognizeResult = await runStage("B3", "recognize", audit, () =>
      recognize({ db: args.db, locus: args.locus, logger }),
    );
    logger.info(
      `  B3 complete — matched: ${recognized.matched}` +
        (recognized.matched ? `, principle: ${recognized.principleId}` : "") +
        ` in ${Date.now() - t0b3}ms`,
    );

    // Stages C1 + C2: formulate invariant and open overlay worktree IN PARALLEL.
    logger.stage("C1+C2: formulateInvariant || openOverlay");
    const t0c1c2 = Date.now();
    const [invariantResult, overlay] = await Promise.all([
      runStage("C1", "formulateInvariant", audit, () =>
        formulateInvariant({
          signal: args.signal,
          locus: args.locus,
          db: args.db,
          llm: args.llm,
          logger,
          recognized,
          investigateReport: args.investigateReport,
        }),
      ),
      runStage("C2", "openOverlay", audit, () =>
        openOverlay({ locus: args.locus, db: args.db }),
      ),
    ]);
    invariant = invariantResult;
    logger.info(`  C1+C2 complete — worktree: ${overlay.worktreePath} in ${Date.now() - t0c1c2}ms (parallel)`);

    // Stage C3: generate fix candidate
    logger.stage("C3: generateFixCandidate");
    const t0c3 = Date.now();
    fix = await runStage("C3", "generateFixCandidate", audit, () =>
      generateFixCandidate({
        signal: args.signal,
        locus: args.locus,
        invariant: invariant!,
        overlay,
        llm: args.llm,
        logger,
        recognized,
        investigateReport: args.investigateReport,
      }),
    );
    logger.info(`  C3 complete — patch files: ${fix.patch.fileEdits.length} invariantHolds: ${fix.invariantHoldsUnderOverlay} in ${Date.now() - t0c3}ms`);

    // Emit a patch artifact as soon as C3 returns. Survives downstream failure.
    artifacts.push({
      kind: "patch",
      patch: fix.patch,
      rationale: fix.llmRationale,
      source: fix.source,
    });

    // Stage C4: generate complementary changes
    logger.stage("C4: generateComplementary");
    const t0c4 = Date.now();
    const complementary = await runStage("C4", "generateComplementary", audit, () =>
      generateComplementary({
        fix: fix!,
        locus: args.locus,
        overlay,
        db: args.db,
        llm: args.llm,
        maxSites: args.options.maxComplementarySites,
        logger,
      }),
    );
    logger.info(`  C4 complete — ${complementary.length} sites in ${Date.now() - t0c4}ms`);

    // Stage C5: generate regression test
    logger.stage("C5: generateRegressionTest");
    const t0c5 = Date.now();
    const c5Result = await runStage("C5", "generateRegressionTest", audit, () =>
      generateRegressionTest({
        fix: fix!,
        signal: args.signal,
        locus: args.locus,
        overlay,
        invariant: invariant!,
        llm: args.llm,
        testRunner: args.c5TestRunner,
        logger,
        recognized,
        investigateReport: args.investigateReport,
      }),
    );
    test = c5Result;
    logger.info(`  C5 complete — passesOnFixed: ${test?.passesOnFixedCode} failsOnOriginal: ${test?.failsOnOriginalCode} in ${Date.now() - t0c5}ms`);

    if (test) {
      artifacts.push({ kind: "regression_test", test });
    }

    // Stage C6: generate principle candidate(s).
    logger.stage("C6: generatePrincipleCandidate");
    const t0c6 = Date.now();
    const principles = await runStage("C6", "generatePrincipleCandidate", audit, () =>
      generatePrincipleCandidate({ signal: args.signal, invariant: invariant!, fixCandidate: fix!, db: args.db, llm: args.llm, overlay, logger, recognized }),
    );
    const primaryPrinciple = principles.length > 0 ? principles[0] : null;
    const alternateShapes = principles.length > 1 ? principles.slice(1) : [];
    logger.info(
      `  C6 complete — primary kind: ${primaryPrinciple?.kind ?? "null"}, ` +
      `${alternateShapes.length} alternate shape(s) in ${Date.now() - t0c6}ms`,
    );

    if (primaryPrinciple) {
      artifacts.push({
        kind: "principle",
        principle: primaryPrinciple,
        alternateShapes: alternateShapes.length > 0 ? alternateShapes : undefined,
      });
    }

    // Stage D1: assemble bundle
    logger.stage("D1: assembleBundle");
    const t0d1 = Date.now();
    const bundle = await runStage("D1", "assembleBundle", audit, () =>
      assembleBundle({
        signal: args.signal,
        plan: args.plan,
        locus: args.locus,
        fix: fix!,
        complementary,
        test,
        principle: primaryPrinciple,
        alternateShapes,
        overlay,
        db: args.db,
        existingAuditTrail: audit,
        vitestRunner: args.vitestRunner,
        logger,
      }),
    );
    logger.info(`  D1 complete — bundleId: ${bundle.bundleId} confidence: ${bundle.confidence.toFixed(2)} in ${Date.now() - t0d1}ms`);

    artifacts.push({ kind: "bundle", bundle });

    // -----------------------------------------------------------------------
    // PERSIST INVARIANT (early, idempotent path).
    //
    // Best-effort: if this throws, the `finally` block tries again. Doing it
    // here lets the standing-runtime spec's contract hold even when D2 is
    // not exercised (e.g., dry-run, autoApply=false).
    // -----------------------------------------------------------------------
    invariantPersisted = await flushInvariantArtifact({
      logger,
      claim: invariant,
      signal: args.signal,
      locus: args.locus,
      fix,
      test,
      patchSha: null,
    }) || invariantPersisted;

    // Stage D2: apply bundle
    logger.stage("D2: applyBundle");
    const t0d2 = Date.now();
    const applyResult = await runStage("D2", "applyBundle", audit, () =>
      applyBundle({ bundle, options: { autoApply: args.options.autoApply, prDraftMode: !args.options.autoApply }, db: args.db, logger }),
    );
    logger.info(`  D2 complete — applied: ${applyResult.applied} commitSha: ${applyResult.commitSha ?? "none"} in ${Date.now() - t0d2}ms`);

    // Stage D3: learn from bundle (only if apply succeeded)
    if (applyResult.applied) {
      logger.stage("D3: learnFromBundle");
      const t0d3 = Date.now();
      await runStage("D3", "learnFromBundle", audit, () =>
        learnFromBundle({ bundle, applyResult, db: args.db, logger }),
      );
      logger.info(`  D3 complete in ${Date.now() - t0d3}ms`);
    }

    result = {
      bundle,
      applied: applyResult.applied,
      auditTrail: audit,
      reason: applyResult.applied ? undefined : applyResult.failureReason,
      applyResult,
      artifacts,
    };
  } catch (err) {
    if (err instanceof NotImplementedError) {
      logger.info(`  stage ${err.stageId} not yet implemented — graceful abort`);
      audit.push({
        stage: err.stageId,
        kind: "skipped",
        detail: err.message,
        timestamp: Date.now(),
      });
      result = {
        bundle: null,
        applied: false,
        auditTrail: audit,
        reason: `aborted at stage ${err.stageId}: ${err.message}`,
        artifacts,
      };
    } else {
      logger.error(`orchestrator caught unexpected error`, {
        message: err instanceof Error ? err.message : String(err),
        auditSoFar: audit.map((e) => `${e.stage}:${e.kind}`),
      });
      audit.push({
        stage: "orchestrator",
        kind: "error",
        detail: err instanceof Error ? err.message : String(err),
        timestamp: Date.now(),
      });
      result = {
        bundle: null,
        applied: false,
        auditTrail: audit,
        reason: err instanceof Error ? err.message : String(err),
        artifacts,
      };
    }
  } finally {
    // -----------------------------------------------------------------------
    // ARTIFACT-STREAM FLUSH (unconditional).
    //
    // Cross-stage flush gating is removed: a downstream failure must NOT
    // suppress the invariant's persistence. If we have an invariant claim
    // (C1 succeeded) and we haven't already written it on the early path,
    // write it now. AC#3 unblock.
    // -----------------------------------------------------------------------
    if (!invariantPersisted && invariant) {
      try {
        await flushInvariantArtifact({
          logger,
          claim: invariant,
          signal: args.signal,
          locus: args.locus,
          fix,
          test,
          patchSha: null,
        });
      } catch (flushErr) {
        const msg = flushErr instanceof Error ? flushErr.message : String(flushErr);
        logger.error(`invariant flush (finally) failed (non-fatal)`, { error: msg });
      }
    }
  }

  return result;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Materialize the invariant claim into a StoredInvariant and write it to
 * `.provekit/invariants/<sha>.json`. Returns true on successful write.
 *
 * Failures are caught + logged; never thrown. The invariant write must not
 * abort the loop (the bundle still ships) — but we DO want the artifact
 * collected in the return value's `artifacts` list when it lands.
 */
async function flushInvariantArtifact(args: {
  logger: FixLoopLogger;
  claim: InvariantClaim;
  signal: BugSignal;
  locus: BugLocus;
  /**
   * C3's patch output. When present, the persisted StoredInvariant's
   * callsite + binding `node` blocks are derived from the patched file
   * (its path + the post-edit line geometry of each `source_expr`).
   * When null (B-stage early failure, or C3 produced no edits), the
   * legacy `locus`-derived shape is used as a graceful fallback.
   */
  fix: FixCandidate | null;
  test: TestArtifact | null;
  patchSha: string | null;
}): Promise<boolean> {
  try {
    const projectRoot = resolveProjectRoot(args.locus.file);
    if (!projectRoot) return false;

    // Issue #138/#139 fix: ground callsite + binding geometry on C3's
    // actual patch (when present), not on Locate's pre-stage best-guess.
    const { callsiteOverride, bindingLocations } = computePatchGeometry(
      args.fix,
      args.claim,
      args.locus,
      args.logger,
    );

    const stored = buildStoredInvariant({
      claim: args.claim,
      signal: args.signal,
      locus: args.locus,
      test: args.test,
      patchSha: args.patchSha,
      bindingNodeHashes: new Map(),
      callsiteOverride,
      bindingLocations,
    });
    const writtenAt = writeInvariant(projectRoot, stored);
    args.logger.info(`  invariant persisted: ${writtenAt}`);
    return true;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    args.logger.error(`invariant store write failed (non-fatal)`, { error: msg });
    return false;
  }
}

/**
 * Build `callsiteOverride` + `bindingLocations` from C3's patch by
 * inspecting the primary patched file's `newContent`.
 *
 * For each binding, locate `source_expr` as a literal substring in the
 * post-edit content and return the matched line range. A binding whose
 * source_expr cannot be located gets startLine=endLine=0 with a debug
 * log — that's an honest "we don't know" so the verify CLI marks the
 * binding as decayed rather than silently persisting a wrong line.
 *
 * Returns `{}` when no patch is available — caller falls back to the
 * legacy locus-derived shape.
 */
function computePatchGeometry(
  fix: FixCandidate | null,
  claim: InvariantClaim,
  locus: BugLocus,
  logger: FixLoopLogger,
): {
  callsiteOverride?: { filePath: string; startLine: number; endLine: number };
  bindingLocations?: Map<
    string,
    { filePath: string; startLine: number; endLine: number }
  >;
} {
  if (!fix) return {};
  const primary = pickPrimaryPatchEdit(fix);
  if (!primary) return {};

  const filePath = primary.file;
  const newContent = primary.newContent ?? "";

  // Per-binding location map. Look up each binding's source_expr in the
  // post-edit content. Honest-zero on miss (no fall-back to the claim's
  // pre-edit source_line guess — that's the very value the verify CLI
  // can't resolve to a substrate node).
  const bindingLocations = new Map<
    string,
    { filePath: string; startLine: number; endLine: number }
  >();
  for (const b of claim.bindings) {
    const lines = findExpressionLines(newContent, b.source_expr);
    if (lines) {
      bindingLocations.set(b.smt_constant, {
        filePath,
        startLine: lines.startLine,
        endLine: lines.endLine,
      });
    } else {
      bindingLocations.set(b.smt_constant, {
        filePath,
        startLine: 0,
        endLine: 0,
      });
      logger.info(
        `  invariant flush: binding "${b.smt_constant}" source_expr not located in patched ${filePath} (post-edit content); persisting startLine=endLine=0`,
      );
    }
  }

  // Callsite line: the function name from Locate, found in the post-edit
  // file. v1 best-effort; if the function name doesn't match anything,
  // try the first binding's resolved line as a representative anchor;
  // last resort, line 1.
  const fnLine = findFunctionLine(newContent, locus.function);
  let callsiteLine: number;
  if (fnLine != null) {
    callsiteLine = fnLine;
  } else {
    if (locus.function) {
      logger.info(
        `  invariant flush: function "${locus.function}" not found in patched file ${filePath}; falling back to first-binding anchor`,
      );
    }
    // Pick the first binding's resolved start line, if any non-zero.
    let anchored = 0;
    for (const loc of bindingLocations.values()) {
      if (loc.startLine > 0) {
        anchored = loc.startLine;
        break;
      }
    }
    callsiteLine = anchored > 0 ? anchored : 1;
  }

  return {
    callsiteOverride: {
      filePath,
      startLine: callsiteLine,
      endLine: callsiteLine,
    },
    bindingLocations,
  };
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
      throw err;
    }
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

/**
 * Find the git repo root that contains the given file. Used to anchor
 * `.provekit/invariants/` writes to the user's project root rather than
 * to the orchestrator's process cwd. Returns null if the file is not
 * inside a git repo or git is unavailable — caller treats null as "skip
 * the persistence step."
 */
function resolveProjectRoot(locusFile: string): string | null {
  try {
    const root = execFileSync(
      "git",
      ["rev-parse", "--show-toplevel"],
      { cwd: dirname(locusFile), encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
    ).trim();
    return root || null;
  } catch {
    return null;
  }
}
