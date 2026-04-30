/**
 * Barrel for the fix-loop context layer.
 *
 * Stage signature pattern:
 *   (ctx: FixLoopContext, rt: FixLoopRuntime) => Promise<TArtifact>
 *
 * Orchestrator pattern:
 *   const ctx2 = (await runStage(ctx, rt, audit, "Investigate",
 *                  "investigateReport", investigate)).ctx;
 *
 * Each artifact lives in its own file under src/fix/context/ to keep
 * the data surface legible. The context type itself is in
 * FixLoopContext.ts; runtime in FixLoopRuntime.ts.
 */

export type { FixLoopContext } from "./FixLoopContext.js";
export { createInitialContext, extendContext } from "./FixLoopContext.js";
export type { FixLoopRuntime } from "./FixLoopRuntime.js";
export { runStage } from "./runStage.js";
export type { AuditEntry, RunStageResult } from "./runStage.js";

export type { BugReport } from "./BugReport.js";
export type {
  InvestigateReport,
  CandidateLocation,
  ConfidenceTier,
} from "./InvestigateReport.js";
export type { LocusReport } from "./LocusReport.js";
export type { ClassifyReport } from "./ClassifyReport.js";
export type { RecognizeReport } from "./RecognizeReport.js";
export type { InvariantReport } from "./InvariantReport.js";
export type { FixCandidateReport } from "./FixCandidateReport.js";
export type {
  ComplementaryReport,
  ComplementarySite,
} from "./ComplementaryReport.js";
export type { RegressionTestReport } from "./RegressionTestReport.js";
export type { PrincipleReport } from "./PrincipleReport.js";
