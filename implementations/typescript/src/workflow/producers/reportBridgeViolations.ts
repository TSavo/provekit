/**
 * report-bridge-violations: Stage 5 (action) of the bridge enforcement
 * workflow.
 *
 * Aggregates per-callsite verdicts from solve-obligation into a unified
 * report. Each callsite contributes a row; rows whose verdict is not
 * "discharged" are surfaced as violations. The report is the
 * verifier's machine-readable output and the LSP's source for
 * diagnostics.
 *
 * This stage is an action (not a property memento): it doesn't
 * produce a CID-addressable claim, just a derived report.
 */

import type { Stage } from "../types.js";
import type { BridgeCallSite } from "./enumerateBridgeCallsites.js";
export type { BridgeCallSite };
import type { ObligationVerdict } from "./solveObligation.js";

export const REPORT_BRIDGE_VIOLATIONS_CAPABILITY = "report-bridge-violations";

export interface BridgeReportRow {
  callsite: BridgeCallSite;
  /** Final verdict for this callsite. */
  status:
    | "discharged"           // bridge target's precondition holds at this call
    | "unsatisfied"          // counter-example exists; precondition can fail
    | "undecidable"          // solver gave up
    | "disagreement"         // multi-entry solver disagreed
    | "unresolved-target"    // bridge.targetContractCid not locally available
    | "non-precondition"     // resolved memento isn't a forall (shape gap)
    | "unliftable-argument"  // arg wasn't a literal; v1 lifter limit
    | "lift-error";          // substitution capture or other instantiation error
  /** Free-form reason supporting the status. */
  reason?: string;
  /** Per-solver detail when status is "discharged" / "unsatisfied" / "undecidable" / "disagreement". */
  solverProbes?: Array<{ solverType: string; probe: string }>;
}

export interface ReportBridgeViolationsInput {
  rows: BridgeReportRow[];
}

export interface ReportBridgeViolationsOutput {
  totalCallsites: number;
  discharged: number;
  violations: number;
  rows: BridgeReportRow[];
}

export interface MakeReportBridgeViolationsStageDeps {
  producerVersion?: string;
}

export function makeReportBridgeViolationsStage(
  deps: MakeReportBridgeViolationsStageDeps = {},
): Stage<ReportBridgeViolationsInput, ReportBridgeViolationsOutput> {
  const producedBy = deps.producerVersion ?? "reportBridgeViolations@v1";

  return {
    name: "reportBridgeViolations",
    producedBy,

    serializeInput(input) {
      return { rowCount: input.rows.length, rows: input.rows };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ReportBridgeViolationsOutput;
    },

    async run(input) {
      let discharged = 0;
      let violations = 0;
      for (const row of input.rows) {
        if (row.status === "discharged") discharged++;
        else violations++;
      }
      return {
        totalCallsites: input.rows.length,
        discharged,
        violations,
        rows: input.rows,
      };
    },
  };
}

/**
 * Helper for the workflow runner: convert a solve-obligation verdict
 * into a BridgeReportRow status.
 */
export function statusFromObligationVerdict(verdict: ObligationVerdict): BridgeReportRow["status"] {
  switch (verdict) {
    case "discharged": return "discharged";
    case "unsatisfied": return "unsatisfied";
    case "undecidable": return "undecidable";
    case "disagreement": return "disagreement";
  }
}
