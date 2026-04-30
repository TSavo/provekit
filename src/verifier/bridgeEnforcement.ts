/**
 * Bridge enforcement runner — composes the 5-stage workflow into a
 * single entry point used by `provekit verify`.
 *
 * Stages (from src/workflow/producers/):
 *   1. load-all-proofs           → unified CID-keyed pool
 *   2. enumerate-bridge-callsites → BridgeCallSite[] from property mementos
 *   3. resolve-bridge-target     → ResolvedProperty per callsite (hash lookup)
 *   4. solve-obligation          → IR formula → SMT-LIB → solver verdict
 *   5. report-bridge-violations  → aggregate
 *
 * Solver pulled from provekit.config.yaml (composite — multiple
 * entries run in parallel, verdict is consensus). IR is the wire
 * format; solvers consume IR via emitSmtLibProblem at the dispatcher
 * edge.
 */

import { loadProvekitConfig, resolveSolverEntries } from "../config/provekitConfig.js";
import { makeLoadAllProofsStage } from "../workflow/producers/loadAllProofs.js";
import { makeEnumerateBridgeCallsitesStage } from "../workflow/producers/enumerateBridgeCallsites.js";
import { makeResolveBridgeTargetStage } from "../workflow/producers/resolveBridgeTarget.js";
import { makeInstantiateObligationStage } from "../workflow/producers/instantiateObligation.js";
import { makeSolveObligationStage } from "../workflow/producers/solveObligation.js";
import {
  makeReportBridgeViolationsStage,
  statusFromObligationVerdict,
  type BridgeReportRow,
  type ReportBridgeViolationsOutput,
} from "../workflow/producers/reportBridgeViolations.js";
import type { Solver } from "../workflow/producers/checkImplication.js";

export interface BridgeEnforcementReport extends ReportBridgeViolationsOutput {
  /** Errors encountered loading .proof files (failed trust root, decode errors, etc.). */
  loadErrors: Array<{ proofFile: string; reason: string }>;
}

/**
 * Run the full bridge enforcement workflow against a project. Returns
 * the aggregated report plus any errors from the .proof load step.
 */
export async function runBridgeEnforcement(projectRoot: string): Promise<BridgeEnforcementReport> {
  const config = loadProvekitConfig(projectRoot);
  const solverEntries = resolveSolverEntries(config);
  const solver: Solver = {
    entries: solverEntries.map((e) => ({
      type: e.type,
      binary: e.binary ?? e.type,
      compiler: e.compiler ?? "smt-lib",
      flags: e.flags,
      timeoutMs: e.timeoutMs,
    })),
  };

  const loadStage = makeLoadAllProofsStage();
  const enumStage = makeEnumerateBridgeCallsitesStage();
  const resolveStage = makeResolveBridgeTargetStage();
  const instantiateStage = makeInstantiateObligationStage();
  const solveStage = makeSolveObligationStage();
  const reportStage = makeReportBridgeViolationsStage();

  const pool = await loadStage.run({ projectRoot });
  const enumResult = await enumStage.run({
    mementoPool: pool.mementoPool,
    bridgesBySymbol: pool.bridgesBySymbol,
  });

  const rows: BridgeReportRow[] = [];
  for (const cs of enumResult.callsites) {
    const resolved = await resolveStage.run({
      bridgeTargetContractCid: cs.bridgeTargetContractCid,
      mementoPool: pool.mementoPool,
    });
    if (!resolved.resolved) {
      rows.push({
        callsite: cs as unknown as BridgeReportRow["callsite"],
        status: "unresolved-target",
        ...(resolved.failureReason ? { reason: resolved.failureReason } : {}),
      });
      continue;
    }
    const arg = cs.argTerms[0];
    if (!arg) {
      rows.push({
        callsite: cs as unknown as BridgeReportRow["callsite"],
        status: "unliftable-argument",
        reason: "no first arg",
      });
      continue;
    }
    const obligation = await instantiateStage.run({
      formula: resolved.resolved.irFormula,
      argTerm: arg,
    });
    if (!obligation.obligation) {
      rows.push({
        callsite: cs as unknown as BridgeReportRow["callsite"],
        status: obligation.failureReason === "formula-not-forall" ? "non-precondition" : "lift-error",
        ...(obligation.failureMessage ? { reason: obligation.failureMessage } : {}),
      });
      continue;
    }
    const solveResult = await solveStage.run({
      obligation: obligation.obligation,
      solver,
    });
    rows.push({
      callsite: cs as unknown as BridgeReportRow["callsite"],
      status: statusFromObligationVerdict(solveResult.verdict),
      solverProbes: solveResult.perEntry.map((e) => ({
        solverType: e.solverType,
        probe: e.probe,
      })),
    });
  }

  const aggregated = await reportStage.run({ rows });
  return { ...aggregated, loadErrors: pool.errors };
}

/**
 * Render a BridgeEnforcementReport for terminal output.
 */
export function formatBridgeEnforcementReport(report: BridgeEnforcementReport): string {
  const lines: string[] = [];
  if (report.totalCallsites === 0 && report.loadErrors.length === 0) {
    return "  no bridge call sites found in .proof files\n";
  }
  lines.push(`  ${report.totalCallsites} bridge call site${report.totalCallsites === 1 ? "" : "s"}: ` +
    `${report.discharged} discharged, ${report.violations} violation${report.violations === 1 ? "" : "s"}`);
  if (report.loadErrors.length > 0) {
    lines.push(`  ${report.loadErrors.length} .proof load error${report.loadErrors.length === 1 ? "" : "s"}:`);
    for (const e of report.loadErrors) lines.push(`    ${e.proofFile}: ${e.reason}`);
  }
  for (const row of report.rows) {
    if (row.status === "discharged") continue;
    const cs = row.callsite as { bridgeIrName: string; propertyName: string; propertyCid: string };
    const reasonSuffix = row.reason ? ` — ${row.reason}` : "";
    lines.push(`    ✗ ${cs.bridgeIrName} in ${cs.propertyName} (${cs.propertyCid.slice(0, 12)}…): ${row.status}${reasonSuffix}`);
  }
  return lines.join("\n") + "\n";
}
