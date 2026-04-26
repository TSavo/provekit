#!/usr/bin/env tsx
/**
 * scripts/harvest-tag-sample-corrective.ts — pull a sample of pending-principle
 * candidates whose diff has at least one deletion. Per the advisor's call:
 * pure-additive fixes (deletions == 0) are bugs where the fix is "write new
 * code", not "correct wrong code". Those are out-of-scope for the principle
 * library; filter them out before spot-checking precision.
 *
 * Usage:
 *   npx tsx scripts/harvest-tag-sample-corrective.ts --n 10
 */

import { existsSync } from "fs";
import { join, dirname } from "path";
import { homedir } from "os";
import { fileURLToPath } from "url";
import { extractBugs } from "../src/fix/harvest/extractBugs.js";
import { openDb } from "../src/db/index.js";

function parseFlag(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return undefined;
  return args[idx + 1];
}

interface TagRow {
  project: string;
  bug_id: string;
  audit_line: string;
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const nFlag = parseFlag(args, "--n");
  const n = nFlag !== undefined ? parseInt(nFlag, 10) : 10;
  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");

  const __filename = fileURLToPath(import.meta.url);
  const __dirname = dirname(__filename);
  const projectRoot = join(__dirname, "..");
  const dbPath = join(projectRoot, ".provekit", "harvest", "harvest.db");
  const db = openDb(dbPath);

  // All pending-principle rows (252 total).
  const rows = db.$client
    .prepare(
      `SELECT project, bug_id, audit_line FROM harvest_expressibility
       WHERE tag = 'expressible-now-pending-principle'`,
    )
    .all() as TagRow[];

  // Pre-load extractBugs to look up stats.
  const projects = Array.from(new Set(rows.map((r) => r.project)));
  const candidatesByKey = new Map<string, { upstreamFixMessage: string; diff: string; stats: { filesChanged: number; insertions: number; deletions: number } }>();
  for (const project of projects) {
    const projectPath = join(bugsDir, project);
    if (!existsSync(join(projectPath, ".git"))) continue;
    const ex = extractBugs({ projectPath, project });
    for (const c of ex.candidates) {
      candidatesByKey.set(`${project}-${c.source.bugId}`, {
        upstreamFixMessage: c.upstreamFixMessage,
        diff: c.diff,
        stats: c.stats,
      });
    }
  }

  // Filter to corrective (deletions >= 1).
  const corrective = rows.filter((r) => {
    const c = candidatesByKey.get(`${r.project}-${r.bug_id}`);
    return c !== undefined && c.stats.deletions >= 1;
  });

  console.log(`# Pending-principle filtered to corrective (deletions >= 1)`);
  console.log(`Total pending-principle: ${rows.length}`);
  console.log(`After filter:           ${corrective.length}`);
  console.log();

  // Random sample of n.
  const shuffled = [...corrective].sort(() => Math.random() - 0.5);
  const sample = shuffled.slice(0, n);

  for (const r of sample) {
    const c = candidatesByKey.get(`${r.project}-${r.bug_id}`);
    const subject = c?.upstreamFixMessage.split("\n")[0] ?? "<missing>";
    const stats = c ? `${c.stats.filesChanged}f +${c.stats.insertions}/-${c.stats.deletions}` : "";
    console.log(`## [${r.project}-${r.bug_id}]  ${stats}`);
    console.log(`subject: ${truncate(subject, 110)}`);
    console.log(`audit:   ${r.audit_line}`);
    if (c) {
      const diffLines = c.diff.split("\n").slice(0, 22);
      console.log("diff (head):");
      for (const l of diffLines) console.log(`  ${l}`);
      console.log("  ...");
    }
    console.log();
  }

  db.$client.close();
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + "...";
}

main().catch((err) => {
  process.stderr.write(`fatal: ${err instanceof Error ? err.stack ?? err.message : String(err)}\n`);
  process.exit(1);
});
