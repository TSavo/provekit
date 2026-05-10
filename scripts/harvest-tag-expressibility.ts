#!/usr/bin/env tsx
/**
 * scripts/harvest-tag-expressibility.ts: run the v1 mechanical tagger across
 * BugsJS candidates, persist results to .provekit/harvest/harvest.db, and dump
 * audit lines to stdout for the manual-sample-30 review gate.
 *
 * Per the spec (docs/plans/2026-04-26-principle-tightening.md): the tagger is
 * the prerequisite for the held-out empirical test of #115. Until tagger
 * precision ≥ 90% on a 30-sample, the held-out denominator is undefined.
 *
 * Usage:
 *   npx tsx scripts/harvest-tag-expressibility.ts                    # full corpus
 *   npx tsx scripts/harvest-tag-expressibility.ts --max 30           # first 30
 *   npx tsx scripts/harvest-tag-expressibility.ts --project express  # one project
 *   npx tsx scripts/harvest-tag-expressibility.ts --project express --bug 1
 *   npx tsx scripts/harvest-tag-expressibility.ts --print-audit      # echo audit lines
 */

import { existsSync, mkdirSync } from "fs";
import { join, dirname } from "path";
import { homedir } from "os";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { sql } from "drizzle-orm";
import { extractBugs, type HarvestCandidate } from "../src/fix/harvest/extractBugs.js";
import { tagExpressibility } from "../src/fix/harvest/expressibility.js";
import { openDb } from "../src/db/index.js";
import { harvestExpressibility } from "../src/db/schema/harvestExpressibility.js";

function parseFlag(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return undefined;
  return args[idx + 1];
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");
  if (!existsSync(bugsDir)) {
    process.stderr.write(`No BugsJS directory at ${bugsDir}.\n`);
    process.exit(1);
  }

  const projectFlag = parseFlag(args, "--project");
  const bugFlag = parseFlag(args, "--bug");
  const maxFlag = parseFlag(args, "--max");
  const max = maxFlag !== undefined ? parseInt(maxFlag, 10) : Infinity;
  const printAudit = args.includes("--print-audit");

  const __filename = fileURLToPath(import.meta.url);
  const __dirname = dirname(__filename);
  const projectRoot = join(__dirname, "..");
  const harvestDir = join(projectRoot, ".provekit", "harvest");
  mkdirSync(harvestDir, { recursive: true });
  const dbPath = join(harvestDir, "harvest.db");

  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: join(projectRoot, "drizzle") });

  const projects = projectFlag
    ? [projectFlag]
    : ["eslint", "express", "karma", "hexo", "hessian.js", "pencilblue", "shields", "bower"]
        .filter((p) => existsSync(join(bugsDir, p, ".git")));

  const candidates: { project: string; candidate: HarvestCandidate }[] = [];
  for (const project of projects) {
    const projectPath = join(bugsDir, project);
    const ex = extractBugs({ projectPath, project, ...(bugFlag ? { onlyBugIds: [bugFlag] } : {}) });
    for (const c of ex.candidates) candidates.push({ project, candidate: c });
  }

  console.log(`# Expressibility tagger v1 (mechanical)`);
  console.log(`projects: ${projects.join(",")}`);
  console.log(`candidates discovered: ${candidates.length}`);
  console.log(`max: ${max === Infinity ? "all" : max}`);
  console.log();

  const counts: Record<string, number> = {};
  let processed = 0;
  for (const { project, candidate } of candidates) {
    if (processed >= max) break;
    processed++;

    const t0 = Date.now();
    const tag = tagExpressibility({ candidate });
    const elapsedMs = Date.now() - t0;

    counts[tag.tag] = (counts[tag.tag] ?? 0) + 1;

    db.insert(harvestExpressibility)
      .values({
        project,
        bugId: candidate.source.bugId,
        tag: tag.tag,
        layer1Recognized: tag.layer1Recognized,
        layer1MatchedPrinciples: JSON.stringify(tag.layer1MatchedPrinciples),
        signatureColumns: JSON.stringify(tag.signatureColumns),
        signatureKinds: JSON.stringify(tag.signatureKinds),
        signatureRelations: JSON.stringify(tag.signatureRelations),
        missingColumns: JSON.stringify(tag.missingColumns),
        missingRelations: JSON.stringify(tag.missingRelations),
        auditLine: tag.auditLine,
        taggerVersion: tag.taggerVersion,
        taggedAt: tag.taggedAt,
      })
      .onConflictDoUpdate({
        target: [harvestExpressibility.project, harvestExpressibility.bugId],
        set: {
          tag: tag.tag,
          layer1Recognized: tag.layer1Recognized,
          layer1MatchedPrinciples: JSON.stringify(tag.layer1MatchedPrinciples),
          signatureColumns: JSON.stringify(tag.signatureColumns),
          signatureKinds: JSON.stringify(tag.signatureKinds),
          signatureRelations: JSON.stringify(tag.signatureRelations),
          missingColumns: JSON.stringify(tag.missingColumns),
          missingRelations: JSON.stringify(tag.missingRelations),
          auditLine: tag.auditLine,
          taggerVersion: tag.taggerVersion,
          taggedAt: tag.taggedAt,
        },
      })
      .run();

    if (printAudit) {
      console.log(`[${project}-bug-${candidate.source.bugId}] (${(elapsedMs / 1000).toFixed(1)}s) ${tag.auditLine}`);
    }
  }

  console.log();
  console.log(`# Distribution (n=${processed})`);
  const total = processed;
  for (const tag of [
    "expressible-now-recognized",
    "expressible-now-pending-principle",
    "needs-capability-extension",
    "needs-new-relation",
    "unknown",
  ]) {
    const n = counts[tag] ?? 0;
    const pct = total > 0 ? ((n / total) * 100).toFixed(1) : "0.0";
    console.log(`  ${tag.padEnd(38)} ${String(n).padStart(4)}  (${pct}%)`);
  }

  console.log();
  console.log(`Persisted to ${dbPath}`);
  console.log(`Query: sqlite3 ${dbPath} "SELECT tag, count(*) FROM harvest_expressibility GROUP BY tag"`);

  // Suppress unused import warning when sql is not invoked.
  void sql;
  db.$client.close();
}

main().catch((err) => {
  process.stderr.write(`fatal: ${err instanceof Error ? err.stack ?? err.message : String(err)}\n`);
  process.exit(1);
});
