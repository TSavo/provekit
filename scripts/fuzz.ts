#!/usr/bin/env ts-node
/**
 * scripts/fuzz.ts — run the corpus sweep and report the dashboard.
 *
 * Usage:
 *   npx ts-node scripts/fuzz.ts [--threshold 0.10] [--bug-class division-by-zero]
 *
 * Exit 0 if integrationGapRate < threshold (default 0.05).
 * Exit 1 if integrationGapRate >= threshold (seams need hardening).
 * Exit 2 on unexpected runner error.
 *
 * Logs each scenario's full run to .provekit/fuzz-runs/<sweepId>/<scenarioId>.log.
 */

import { runSweep } from "../src/fix/corpus/runner.js";
import { summarize, formatDashboardForCLI } from "../src/fix/corpus/dashboard.js";
import { loadScenarios } from "../src/fix/corpus/index.js";

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  // Parse --threshold N
  const thresholdIdx = args.indexOf("--threshold");
  const threshold = thresholdIdx !== -1 && thresholdIdx + 1 < args.length
    ? parseFloat(args[thresholdIdx + 1])
    : 0.05;

  // Parse --bug-class <class>
  const bugClassIdx = args.indexOf("--bug-class");
  const bugClass = bugClassIdx !== -1 && bugClassIdx + 1 < args.length
    ? args[bugClassIdx + 1]
    : "all";

  const corpus = loadScenarios(bugClass);
  if (corpus.length === 0) {
    process.stderr.write(`No scenarios found for bug class '${bugClass}'.\n`);
    process.exit(2);
  }

  process.stdout.write(`Running corpus sweep: ${corpus.length} scenario(s), threshold=${(threshold * 100).toFixed(1)}%\n\n`);

  let results;
  try {
    results = await runSweep(corpus);
  } catch (err) {
    process.stderr.write(`Sweep failed: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }

  const dashboard = summarize(results);
  const formatted = formatDashboardForCLI(dashboard);
  process.stdout.write(formatted + "\n");

  // Per-scenario summary
  process.stdout.write("\nPer-scenario results:\n");
  for (const r of results) {
    const icon = r.classification === "match" || r.classification === "expected_failure" ? "OK" : "!!";
    process.stdout.write(
      `  [${icon}] ${r.scenarioId.padEnd(30)} ${r.classification.padEnd(20)} outcome=${r.actual.outcome}\n`,
    );
    if (r.actual.failedStage) {
      const reason = r.actual.failureReason
        ? r.actual.failureReason.slice(0, 80)
        : "(no reason)";
      process.stdout.write(`       failed at ${r.actual.failedStage}: ${reason}\n`);
    }
  }

  process.stdout.write("\n");

  if (dashboard.integrationGapRate >= threshold) {
    process.stdout.write(
      `FAIL: integrationGapRate ${(dashboard.integrationGapRate * 100).toFixed(1)}% >= threshold ${(threshold * 100).toFixed(1)}%\n`,
    );
    process.exit(1);
  } else {
    process.stdout.write(
      `PASS: integrationGapRate ${(dashboard.integrationGapRate * 100).toFixed(1)}% < threshold ${(threshold * 100).toFixed(1)}%\n`,
    );
    process.exit(0);
  }
}

main().catch((err) => {
  process.stderr.write(`Unexpected error: ${err instanceof Error ? err.message : String(err)}\n`);
  process.exit(2);
});
