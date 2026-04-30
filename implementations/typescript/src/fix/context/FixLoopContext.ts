/**
 * Immutable artifact bag for the fix loop.
 *
 * The pipeline is a series of stages, each producing one artifact.
 * The orchestrator threads a context object through them: every stage
 * sees ALL prior artifacts, never mutates them, and contributes its
 * own. By the time C3 runs, the context carries Intake's BugReport,
 * Investigate's InvestigateReport (if it fired), Locate's LocusReport,
 * Classify's ClassifyReport, and B3's RecognizeReport. C3's prompt
 * builder reads what it needs and self-calibrates from the upstream
 * confidences and rationales.
 *
 * Why immutable: pre-refactor (early 2026-04-27), Investigate mutated
 * `signal.codeReferences` to feed Locate. Downstream stages couldn't
 * tell whether the references they saw came from Intake or Investigate
 * — the mutation hid provenance. With immutable artifacts, every stage
 * names its source: `ctx.bugReport.signal.codeReferences` vs
 * `ctx.investigateReport.candidateLocations`. Provenance is type-level.
 *
 * Why one big bag instead of selective threading: pre-refactor, C3 took
 * `(signal, locus, invariant, ...)` in its function signature. Adding
 * a new artifact required touching every call site. With one ctx, new
 * artifacts are added by extending the type — call sites are unchanged.
 *
 * The bag grows monotonically: each stage's runStage call returns
 * `{ ...prevCtx, [stageKey]: newArtifact }`. TypeScript's `readonly`
 * modifiers prevent rewrites; only fresh ctx instances are constructed.
 */

import type { BugReport } from "./BugReport.js";
import type { InvestigateReport } from "./InvestigateReport.js";
import type { LocusReport } from "./LocusReport.js";
import type { ClassifyReport } from "./ClassifyReport.js";
import type { RecognizeReport } from "./RecognizeReport.js";
import type { InvariantReport } from "./InvariantReport.js";
import type { FixCandidateReport } from "./FixCandidateReport.js";
import type { ComplementaryReport } from "./ComplementaryReport.js";
import type { RegressionTestReport } from "./RegressionTestReport.js";
import type { PrincipleReport } from "./PrincipleReport.js";
import type { OverlayHandle } from "../types.js";

export interface FixLoopContext {
  /** Run identifier — used to scope per-run artifact directory. */
  readonly runId: string;

  // ── Discovery + planning artifacts (always populated by the time C-stages run) ──

  /** Intake's parsed signal + confidence. Always populated. */
  readonly bugReport: BugReport;

  /**
   * Investigate's candidate code sites, root-cause hypothesis, fix
   * hypothesis. Populated when Intake didn't produce usable code refs.
   * When undefined, Locate worked from BugReport.signal.codeReferences only.
   */
  readonly investigateReport?: InvestigateReport;

  /** Locate's resolved SAST node + match mechanism. Required for C-stages. */
  readonly locusReport?: LocusReport;

  /** Classify's remediation plan + layer choice. Required for C-stages. */
  readonly classifyReport?: ClassifyReport;

  // ── B3 + C-stage artifacts (populated as the loop progresses) ──

  /** Recognize-stage result: matched/unmatched + recognized principle if any. */
  readonly recognizeReport?: RecognizeReport;

  /** C1 invariant + scope + root-cause coverage. */
  readonly invariantReport?: InvariantReport;

  /**
   * C2 overlay worktree. NOT immutable in the data sense — it's a
   * filesystem handle whose contents change as patches are applied —
   * but the handle reference itself is set once and not rewritten.
   */
  readonly overlayHandle?: OverlayHandle;

  /** C3 fix candidate + locus-honored flag + optional disagreement. */
  readonly fixCandidateReport?: FixCandidateReport;

  /** C4 complementary sites. May be empty array (the bug was site-unique). */
  readonly complementaryReport?: ComplementaryReport;

  /** C5 regression test + reproduction-scale assertion. */
  readonly regressionTestReport?: RegressionTestReport;

  /** C6 principle candidates with per-candidate scope + confidence. */
  readonly principleReport?: PrincipleReport;
}

/**
 * Construct the initial context with just the runId + BugReport.
 * Subsequent fields are added by stages via the orchestrator's runStage
 * helper.
 */
export function createInitialContext(args: {
  runId: string;
  bugReport: BugReport;
}): FixLoopContext {
  return {
    runId: args.runId,
    bugReport: args.bugReport,
  };
}

/**
 * Type-safe context extension. Returns a new context with one additional
 * artifact field set; the prior context is unchanged. All readonly
 * constraints are preserved by TypeScript.
 *
 * Used by the orchestrator's runStage helper:
 *   const ctx2 = extendContext(ctx, "investigateReport", report);
 */
export function extendContext<K extends Exclude<keyof FixLoopContext, "runId" | "bugReport">>(
  ctx: FixLoopContext,
  key: K,
  value: NonNullable<FixLoopContext[K]>,
): FixLoopContext {
  return { ...ctx, [key]: value };
}
