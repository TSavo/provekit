#!/usr/bin/env tsx
/**
 * scripts/harvest-recognize-report.ts: for each cloned BugsJS project,
 * extract HarvestCandidates (Phase 1) and run recognition (Phase 2-A) against
 * the current principle library. Inspection-only.
 *
 * The headline number this report produces: of the candidates that survive
 * Phase 1 filters, how many are already covered by an existing principle?
 * That ratio determines how many candidates would need Phase 2-B (discovery)
 *: the expensive LLM path.
 *
 * Usage:
 *   npx tsx scripts/harvest-recognize-report.ts                # default ~/bugsjs, all projects
 *   npx tsx scripts/harvest-recognize-report.ts --max 30       # cap per-project
 *   npx tsx scripts/harvest-recognize-report.ts --only express # one project
 */

import { existsSync, readdirSync, statSync } from "fs";
import { join } from "path";
import { homedir } from "os";
import { extractBugs } from "../src/fix/harvest/extractBugs.js";
import { recognizeCandidate } from "../src/fix/harvest/recognize.js";

function parseFlag(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return undefined;
  return args[idx + 1];
}

function main(): void {
  const args = process.argv.slice(2);
  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");
  if (!existsSync(bugsDir)) {
    process.stderr.write(`No BugsJS directory at ${bugsDir}.\n`);
    process.exit(1);
  }

  const maxBugs = parseFlag(args, "--max");
  const onlyProject = parseFlag(args, "--only");

  const projects = readdirSync(bugsDir)
    .filter((entry) => existsSync(join(bugsDir, entry, ".git")) && statSync(join(bugsDir, entry)).isDirectory())
    .filter((entry) => onlyProject === undefined || entry === onlyProject)
    .sort();

  const opts = {
    maxBugs: maxBugs !== undefined ? parseInt(maxBugs, 10) : undefined,
  };

  console.log(`# Harvest recognition report`);
  console.log(`bugsDir=${bugsDir}`);
  console.log(`maxBugs=${opts.maxBugs ?? "all"} onlyProject=${onlyProject ?? "all"}`);
  console.log();

  const totalsByPrinciple = new Map<string, number>();
  let totalCandidates = 0;
  let totalRecognized = 0;
  let totalNeedsDiscovery = 0;

  for (const project of projects) {
    const projectPath = join(bugsDir, project);
    const ex = extractBugs({ projectPath, project, ...opts });
    let recognizedHere = 0;
    let discoveryHere = 0;
    const projectPrincipleHits = new Map<string, number>();

    for (const candidate of ex.candidates) {
      const r = recognizeCandidate(candidate);
      if (r.recognized) {
        recognizedHere++;
        // Count unique principles that fired (one principle per candidate counted
        // once even if it matched multiple lines).
        const hit = new Set<string>();
        for (const m of r.matches) hit.add(m.principleName);
        for (const p of hit) {
          projectPrincipleHits.set(p, (projectPrincipleHits.get(p) ?? 0) + 1);
          totalsByPrinciple.set(p, (totalsByPrinciple.get(p) ?? 0) + 1);
        }
      } else {
        discoveryHere++;
      }
    }

    totalCandidates += ex.candidates.length;
    totalRecognized += recognizedHere;
    totalNeedsDiscovery += discoveryHere;

    const recPct = ex.candidates.length === 0 ? "n/a" : `${((recognizedHere / ex.candidates.length) * 100).toFixed(0)}%`;
    console.log(
      `## ${project}: ${recognizedHere} recognized / ${ex.candidates.length} candidates (${recPct})`,
    );
    if (projectPrincipleHits.size > 0) {
      const sorted = Array.from(projectPrincipleHits.entries()).sort((a, b) => b[1] - a[1]);
      for (const [name, count] of sorted) {
        console.log(`     ${count.toString().padStart(4)}  ${name}`);
      }
    }
  }

  console.log();
  console.log(`# Totals`);
  console.log(`candidates: ${totalCandidates}`);
  console.log(`recognized (would skip discovery): ${totalRecognized}`);
  console.log(`needs discovery (Phase 2-B LLM): ${totalNeedsDiscovery}`);
  if (totalCandidates > 0) {
    console.log(`recognition coverage: ${((totalRecognized / totalCandidates) * 100).toFixed(1)}%`);
  }

  if (totalsByPrinciple.size > 0) {
    console.log();
    console.log(`# Principle hit counts (across all projects)`);
    const sorted = Array.from(totalsByPrinciple.entries()).sort((a, b) => b[1] - a[1]);
    for (const [name, count] of sorted) {
      console.log(`  ${count.toString().padStart(5)}  ${name}`);
    }
  }
}

main();
