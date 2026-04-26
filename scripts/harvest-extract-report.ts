#!/usr/bin/env tsx
/**
 * scripts/harvest-extract-report.ts — run extractBugs against each BugsJS
 * project clone and print survival stats. Inspection-only; no LLM, no library
 * writes. Used to calibrate filters before running the full harvest pipeline.
 *
 * Usage:
 *   npx tsx scripts/harvest-extract-report.ts                    # default ~/bugsjs
 *   BUGSJS_DIR=/path npx tsx scripts/harvest-extract-report.ts   # alt root
 *   npx tsx scripts/harvest-extract-report.ts --max 10           # cap per-project
 *   npx tsx scripts/harvest-extract-report.ts --maxFiles 3 --maxLoc 100
 */

import { existsSync, readdirSync, statSync } from "fs";
import { join } from "path";
import { homedir } from "os";
import { extractBugs } from "../src/fix/harvest/extractBugs.js";

function parseFlag(args: string[], name: string, defaultVal?: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return defaultVal;
  return args[idx + 1];
}

function main(): void {
  const args = process.argv.slice(2);
  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");
  if (!existsSync(bugsDir)) {
    process.stderr.write(`No BugsJS directory at ${bugsDir}. Run scripts/clone-bugsjs.sh first.\n`);
    process.exit(1);
  }

  const maxBugs = parseFlag(args, "--max");
  const maxFiles = parseFlag(args, "--maxFiles");
  const maxLoc = parseFlag(args, "--maxLoc");

  const projects = readdirSync(bugsDir)
    .filter((entry) => {
      const p = join(bugsDir, entry, ".git");
      return existsSync(p) && statSync(join(bugsDir, entry)).isDirectory();
    })
    .sort();

  const opts = {
    maxFiles: maxFiles !== undefined ? parseInt(maxFiles, 10) : undefined,
    maxLoc: maxLoc !== undefined ? parseInt(maxLoc, 10) : undefined,
    maxBugs: maxBugs !== undefined ? parseInt(maxBugs, 10) : undefined,
  };

  // Header
  console.log(`# Harvest Phase 1 extraction report`);
  console.log(`bugsDir=${bugsDir}`);
  console.log(`maxFiles=${opts.maxFiles ?? 2} maxLoc=${opts.maxLoc ?? 50} maxBugs=${opts.maxBugs ?? "all"}`);
  console.log();

  let totalBugs = 0;
  let totalCandidates = 0;
  let totalSkipped = 0;
  const skipReasonCounts = new Map<string, number>();

  for (const project of projects) {
    const projectPath = join(bugsDir, project);
    const result = extractBugs({ projectPath, project, ...opts });
    totalBugs += result.totalBugIds;
    totalCandidates += result.candidates.length;
    totalSkipped += result.skipped.length;

    // Bucket skip reasons by leading clause.
    for (const s of result.skipped) {
      const bucket = s.reason.split(/[:(]/)[0]!.trim();
      skipReasonCounts.set(bucket, (skipReasonCounts.get(bucket) ?? 0) + 1);
    }

    const yieldPct = result.totalBugIds === 0
      ? "n/a"
      : `${((result.candidates.length / result.totalBugIds) * 100).toFixed(0)}%`;
    console.log(
      `## ${project}: ${result.candidates.length} / ${result.totalBugIds} candidates ` +
      `(${yieldPct} yield, ${result.skipped.length} skipped)`,
    );
  }

  console.log();
  console.log(`# Totals`);
  console.log(`bugs enumerated: ${totalBugs}`);
  console.log(`candidates produced: ${totalCandidates}`);
  console.log(`skipped: ${totalSkipped}`);
  if (totalBugs > 0) {
    console.log(`overall yield: ${((totalCandidates / totalBugs) * 100).toFixed(1)}%`);
  }

  if (skipReasonCounts.size > 0) {
    console.log();
    console.log(`# Skip reasons`);
    const sorted = Array.from(skipReasonCounts.entries()).sort((a, b) => b[1] - a[1]);
    for (const [reason, count] of sorted) {
      console.log(`  ${count.toString().padStart(5)}  ${reason}`);
    }
  }
}

main();
