#!/usr/bin/env ts-node
/**
 * scripts/fuzz.ts — run the corpus sweep and report the dashboard.
 *
 * Usage:
 *   npx ts-node scripts/fuzz.ts [--threshold 0.10] [--bug-class division-by-zero]
 *                               [--amplify=stryker [--multiplier=3]]
 *
 * Exit 0 if integrationGapRate < threshold (default 0.05).
 * Exit 1 if integrationGapRate >= threshold (seams need hardening).
 * Exit 2 on unexpected runner error.
 *
 * Logs each scenario's full run to .provekit/fuzz-runs/<sweepId>/<scenarioId>.log.
 */

import { runSweep } from "../src/fix/corpus/runner.js";
import { summarize, formatDashboardForCLI } from "../src/fix/corpus/dashboard.js";
import type { AmplificationContext } from "../src/fix/corpus/dashboard.js";
import { loadScenarios } from "../src/fix/corpus/index.js";
import { amplifyScenario } from "../src/fix/corpus/amplify-stryker.js";
import type { CorpusScenario } from "../src/fix/corpus/scenarios.js";

/** Parse `--key=value` or `--key value` style. Returns undefined if absent. */
function parseArg(args: string[], key: string): string | undefined {
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === `--${key}`) {
      return i + 1 < args.length ? args[i + 1] : undefined;
    }
    if (a.startsWith(`--${key}=`)) {
      return a.slice(`--${key}=`.length);
    }
  }
  return undefined;
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  const thresholdRaw = parseArg(args, "threshold");
  const threshold = thresholdRaw !== undefined ? parseFloat(thresholdRaw) : 0.05;

  const bugClass = parseArg(args, "bug-class") ?? "all";

  const amplifyMode = parseArg(args, "amplify");
  const multiplierRaw = parseArg(args, "multiplier");
  const multiplier = multiplierRaw !== undefined ? parseInt(multiplierRaw, 10) : 3;

  const baseCorpus = loadScenarios(bugClass);
  if (baseCorpus.length === 0) {
    process.stderr.write(`No scenarios found for bug class '${bugClass}'.\n`);
    process.exit(2);
  }

  // Build run corpus: base + (optional) amplified scenarios.
  let corpus: CorpusScenario[] = [...baseCorpus];
  let amplificationCtx: AmplificationContext | undefined;

  if (amplifyMode === "stryker") {
    const ctxMap = new Map<string, { baseId: string; mutationKind: string }>();
    let amplifiedCount = 0;
    for (const base of baseCorpus) {
      const amplified = amplifyScenario(base, { maxMutations: multiplier });
      for (const a of amplified) {
        corpus.push(a);
        ctxMap.set(a.id, { baseId: a.baseScenarioId, mutationKind: a.mutationKind });
        amplifiedCount++;
      }
    }
    amplificationCtx = { mutated: ctxMap };
    process.stdout.write(
      `Amplifier: stryker (multiplier=${multiplier}) added ${amplifiedCount} scenario(s) from ${baseCorpus.length} base(s).\n`,
    );
  }

  process.stdout.write(`Running corpus sweep: ${corpus.length} scenario(s), threshold=${(threshold * 100).toFixed(1)}%\n\n`);

  let results;
  try {
    results = await runSweep(corpus);
  } catch (err) {
    process.stderr.write(`Sweep failed: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }

  const dashboard = summarize(results, amplificationCtx);
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
