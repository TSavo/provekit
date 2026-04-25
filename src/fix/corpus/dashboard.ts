/**
 * Corpus sweep dashboard: aggregate per-stage failure rates from a sweep.
 */

import type { SweepResult } from "./runner.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/**
 * A principle-overfit pair: the base scenario classified cleanly but a
 * mutation derived from that base did not. This is what the Stryker
 * amplifier actually surfaces for the user — base passes, variant fails.
 */
export interface PrincipleOverfit {
  baseId: string;
  mutatedId: string;
  mutationKind: string;
  baseClassification: SweepResult["classification"];
  mutatedClassification: SweepResult["classification"];
  mutatedFailedStage?: string;
  mutatedFailureReason?: string;
}

export interface SweepDashboard {
  totalScenarios: number;
  classification: Record<SweepResult["classification"], number>;
  perStageFailures: Record<string, { count: number; topReasons: string[] }>;
  integrationGapRate: number;   // integration_gap / totalScenarios
  successRate: number;          // (match + expected_failure) / totalScenarios
  /** Populated when amplification metadata is supplied to summarize(). */
  principleOverfits?: PrincipleOverfit[];
}

// ---------------------------------------------------------------------------
// summarize
// ---------------------------------------------------------------------------

/**
 * Optional amplification map: amplified-scenario id => { baseId, mutationKind }.
 * When supplied, summarize() computes the principleOverfits list.
 */
export interface AmplificationContext {
  /** Map of mutated scenario id -> base id + kind. */
  mutated: Map<string, { baseId: string; mutationKind: string }>;
}

export function summarize(
  results: SweepResult[],
  amplification?: AmplificationContext,
): SweepDashboard {
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

  // Compute principle-overfit pairs when amplification metadata is provided.
  let principleOverfits: PrincipleOverfit[] | undefined;
  if (amplification) {
    const byId = new Map<string, SweepResult>();
    for (const r of results) byId.set(r.scenarioId, r);

    principleOverfits = [];
    const cleanBase = (cls: SweepResult["classification"]) =>
      cls === "match" || cls === "expected_failure";
    const isFailure = (cls: SweepResult["classification"]) =>
      cls === "integration_gap" || cls === "principle_rejection" || cls === "unknown";

    for (const r of results) {
      const meta = amplification.mutated.get(r.scenarioId);
      if (!meta) continue;
      const baseRes = byId.get(meta.baseId);
      if (!baseRes) continue;
      if (cleanBase(baseRes.classification) && isFailure(r.classification)) {
        principleOverfits.push({
          baseId: meta.baseId,
          mutatedId: r.scenarioId,
          mutationKind: meta.mutationKind,
          baseClassification: baseRes.classification,
          mutatedClassification: r.classification,
          mutatedFailedStage: r.actual.failedStage,
          mutatedFailureReason: r.actual.failureReason,
        });
      }
    }
  }

  return {
    totalScenarios: total,
    classification,
    perStageFailures,
    integrationGapRate,
    successRate,
    ...(principleOverfits ? { principleOverfits } : {}),
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

  // Principle-overfit pairs are the headline output of the Stryker amplifier.
  // Surface them prominently rather than burying them in per-stage counts.
  if (d.principleOverfits) {
    lines.push("");
    lines.push("━━━ Principle Overfit Pairs (base passed, mutation failed) ━━━");
    if (d.principleOverfits.length === 0) {
      lines.push("  None. Every mutation that the base classified cleanly was also classified cleanly.");
    } else {
      lines.push(`  ${d.principleOverfits.length} pair(s) flagged:`);
      for (const o of d.principleOverfits) {
        lines.push(
          `    [${o.baseId}] (${o.baseClassification}) -> [${o.mutatedId}] (${o.mutatedClassification}) via ${o.mutationKind}`,
        );
        if (o.mutatedFailedStage) {
          const reason = o.mutatedFailureReason
            ? o.mutatedFailureReason.slice(0, 100)
            : "(no reason)";
          lines.push(`      failed at ${o.mutatedFailedStage}: ${reason}`);
        }
      }
    }
  }

  return lines.join("\n");
}
