/**
 * Corpus sweep dashboard: aggregate per-stage failure rates from a sweep.
 */

import type { SweepResult } from "./runner.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface SweepDashboard {
  totalScenarios: number;
  classification: Record<SweepResult["classification"], number>;
  perStageFailures: Record<string, { count: number; topReasons: string[] }>;
  integrationGapRate: number;   // integration_gap / totalScenarios
  successRate: number;          // (match + expected_failure) / totalScenarios
}

// ---------------------------------------------------------------------------
// summarize
// ---------------------------------------------------------------------------

export function summarize(results: SweepResult[]): SweepDashboard {
  const total = results.length;

  const classification: Record<SweepResult["classification"], number> = {
    match: 0,
    expected_failure: 0,
    integration_gap: 0,
    principle_rejection: 0,
    unknown: 0,
  };

  const stageFailures: Record<string, { count: number; reasons: string[] }> = {};

  for (const r of results) {
    classification[r.classification]++;

    // Accumulate per-stage failure data from the audit trail.
    for (const entry of r.actual.auditTrail) {
      if (entry.kind === "error" || entry.kind === "skipped") {
        const s = entry.stage;
        if (!stageFailures[s]) stageFailures[s] = { count: 0, reasons: [] };
        stageFailures[s].count++;
        if (entry.detail && !stageFailures[s].reasons.includes(entry.detail)) {
          stageFailures[s].reasons.push(entry.detail);
        }
      }
    }

    // Also accumulate failedStage from the outer result if not already in audit.
    if (r.actual.failedStage) {
      const s = r.actual.failedStage;
      if (!stageFailures[s]) stageFailures[s] = { count: 0, reasons: [] };
      // Don't double-count if audit already recorded it.
      const alreadyCounted = r.actual.auditTrail.some(
        (e) => e.stage === s && (e.kind === "error" || e.kind === "skipped"),
      );
      if (!alreadyCounted) {
        stageFailures[s].count++;
        if (r.actual.failureReason && !stageFailures[s].reasons.includes(r.actual.failureReason)) {
          stageFailures[s].reasons.push(r.actual.failureReason);
        }
      }
    }
  }

  // Build perStageFailures with topReasons capped at 3.
  const perStageFailures: SweepDashboard["perStageFailures"] = {};
  for (const [stage, data] of Object.entries(stageFailures)) {
    perStageFailures[stage] = {
      count: data.count,
      topReasons: data.reasons.slice(0, 3),
    };
  }

  const integrationGapRate = total > 0 ? classification.integration_gap / total : 0;
  const successRate = total > 0 ? (classification.match + classification.expected_failure) / total : 0;

  return {
    totalScenarios: total,
    classification,
    perStageFailures,
    integrationGapRate,
    successRate,
  };
}

// ---------------------------------------------------------------------------
// formatDashboardForCLI
// ---------------------------------------------------------------------------

export function formatDashboardForCLI(d: SweepDashboard): string {
  const lines: string[] = [];

  lines.push("━━━ Corpus Sweep Dashboard ━━━");
  lines.push(`  Total scenarios:     ${d.totalScenarios}`);
  lines.push(`  Success rate:        ${(d.successRate * 100).toFixed(1)}%`);
  lines.push(`  Integration gap rate: ${(d.integrationGapRate * 100).toFixed(1)}%`);
  lines.push("");
  lines.push("  Classifications:");
  lines.push(`    match:               ${d.classification.match}`);
  lines.push(`    expected_failure:    ${d.classification.expected_failure}`);
  lines.push(`    integration_gap:     ${d.classification.integration_gap}`);
  lines.push(`    principle_rejection: ${d.classification.principle_rejection}`);
  lines.push(`    unknown:             ${d.classification.unknown}`);

  const stageKeys = Object.keys(d.perStageFailures).sort();
  if (stageKeys.length > 0) {
    lines.push("");
    lines.push("  Per-stage failures:");
    for (const stage of stageKeys) {
      const { count, topReasons } = d.perStageFailures[stage];
      lines.push(`    ${stage}: ${count} failure(s)`);
      for (const reason of topReasons) {
        const truncated = reason.length > 100 ? reason.slice(0, 97) + "..." : reason;
        lines.push(`      - ${truncated}`);
      }
    }
  }

  lines.push("");
  lines.push(
    d.integrationGapRate === 0
      ? "  No integration gaps detected."
      : `  Integration gap rate ${(d.integrationGapRate * 100).toFixed(1)}% — seams need hardening.`,
  );

  return lines.join("\n");
}
