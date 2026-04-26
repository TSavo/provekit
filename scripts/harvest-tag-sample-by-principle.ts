#!/usr/bin/env tsx
/**
 * scripts/harvest-tag-sample-by-principle.ts — pull a sample of recognized
 * candidates that match a specific principle. For precision spot-checks.
 *
 * Usage:
 *   npx tsx scripts/harvest-tag-sample-by-principle.ts --principle truthy-test-loses-falsy --n 10
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
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const principleFilter = parseFlag(args, "--principle");
  const nFlag = parseFlag(args, "--n");
  const n = nFlag !== undefined ? parseInt(nFlag, 10) : 10;
  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");

  if (!principleFilter) {
    process.stderr.write("--principle <name> required\n");
    process.exit(1);
  }

  const __filename = fileURLToPath(import.meta.url);
  const __dirname = dirname(__filename);
  const projectRoot = join(__dirname, "..");
  const dbPath = join(projectRoot, ".provekit", "harvest", "harvest.db");
  const db = openDb(dbPath);

  const rows = db.$client
    .prepare(
      `SELECT he.project, he.bug_id FROM harvest_expressibility he,
       json_each(he.layer1_matched_principles)
       WHERE he.tag = 'expressible-now-recognized' AND json_each.value = ?`,
    )
    .all(principleFilter) as TagRow[];

  console.log(`# Hits for principle '${principleFilter}'`);
  console.log(`Total: ${rows.length}`);
  console.log();

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

  const shuffled = [...rows].sort(() => Math.random() - 0.5);
  const sample = shuffled.slice(0, n);

  for (const r of sample) {
    const c = candidatesByKey.get(`${r.project}-${r.bug_id}`);
    const subject = c?.upstreamFixMessage.split("\n")[0] ?? "<missing>";
    const stats = c ? `${c.stats.filesChanged}f +${c.stats.insertions}/-${c.stats.deletions}` : "";
    console.log(`## [${r.project}-${r.bug_id}]  ${stats}`);
    console.log(`subject: ${truncate(subject, 110)}`);
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
