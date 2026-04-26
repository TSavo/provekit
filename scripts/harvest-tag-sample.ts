#!/usr/bin/env tsx
/**
 * scripts/harvest-tag-sample.ts — print a stratified sample of expressibility
 * tags for the manual-precision review gate.
 *
 * Per the tightening spec: tagger v1 must hit ≥ 90% precision on a 30-sample
 * before the held-out denominator is trusted. This script picks N tags per
 * bucket, prints the audit line + the upstream commit subject + the diff
 * head so a human can quickly check whether the bucket is plausible.
 *
 * Usage:
 *   npx tsx scripts/harvest-tag-sample.ts --per-bucket 6
 *   npx tsx scripts/harvest-tag-sample.ts --bucket needs-new-relation --n 10
 */

import { existsSync } from "fs";
import { join, dirname } from "path";
import { homedir } from "os";
import { fileURLToPath } from "url";
import { extractBugs } from "../src/fix/harvest/extractBugs.js";
import { openDb } from "../src/db/index.js";
import { sql } from "drizzle-orm";

function parseFlag(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return undefined;
  return args[idx + 1];
}

const BUCKETS = [
  "expressible-now-recognized",
  "expressible-now-pending-principle",
  "needs-capability-extension",
  "needs-new-relation",
  "unknown",
];

interface TagRow {
  project: string;
  bug_id: string;
  tag: string;
  audit_line: string;
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");

  const perBucketFlag = parseFlag(args, "--per-bucket");
  const bucketFilter = parseFlag(args, "--bucket");
  const nFlag = parseFlag(args, "--n");
  const perBucket = perBucketFlag !== undefined ? parseInt(perBucketFlag, 10) : 6;
  const n = nFlag !== undefined ? parseInt(nFlag, 10) : perBucket;

  const __filename = fileURLToPath(import.meta.url);
  const __dirname = dirname(__filename);
  const projectRoot = join(__dirname, "..");
  const dbPath = join(projectRoot, ".provekit", "harvest", "harvest.db");
  if (!existsSync(dbPath)) {
    process.stderr.write(`No harvest.db at ${dbPath}. Run harvest-tag-expressibility first.\n`);
    process.exit(1);
  }

  const db = openDb(dbPath);
  const buckets = bucketFilter ? [bucketFilter] : BUCKETS;

  const rows: TagRow[] = [];
  for (const bucket of buckets) {
    const sampleSize = bucketFilter ? n : perBucket;
    const result = db.$client
      .prepare(
        `SELECT project, bug_id, tag, audit_line FROM harvest_expressibility
         WHERE tag = ? ORDER BY random() LIMIT ?`,
      )
      .all(bucket, sampleSize) as TagRow[];
    rows.push(...result);
  }

  // Pre-load extractBugs results per project so we can look up commit subjects + diffs.
  const projects = Array.from(new Set(rows.map((r) => r.project)));
  const candidatesByProject = new Map<string, Map<string, { upstreamFixMessage: string; diff: string; stats: { filesChanged: number; insertions: number; deletions: number } }>>();
  for (const project of projects) {
    const projectPath = join(bugsDir, project);
    if (!existsSync(join(projectPath, ".git"))) continue;
    const ex = extractBugs({ projectPath, project });
    const byBugId = new Map<string, { upstreamFixMessage: string; diff: string; stats: { filesChanged: number; insertions: number; deletions: number } }>();
    for (const c of ex.candidates) {
      byBugId.set(c.source.bugId, {
        upstreamFixMessage: c.upstreamFixMessage,
        diff: c.diff,
        stats: c.stats,
      });
    }
    candidatesByProject.set(project, byBugId);
  }

  console.log(`# Stratified sample of expressibility tags`);
  console.log(`per-bucket: ${bucketFilter ? n : perBucket}`);
  console.log();

  for (const r of rows) {
    const c = candidatesByProject.get(r.project)?.get(r.bug_id);
    const subject = c?.upstreamFixMessage.split("\n")[0] ?? "<not in current corpus>";
    const stats = c ? `${c.stats.filesChanged}f +${c.stats.insertions}/-${c.stats.deletions}` : "";
    console.log(`## [${r.tag}] ${r.project}-bug-${r.bug_id}  ${stats}`);
    console.log(`subject: ${truncate(subject, 100)}`);
    console.log(`audit:   ${r.audit_line}`);
    if (c) {
      const diffLines = c.diff.split("\n").slice(0, 18);
      console.log("diff (head):");
      for (const l of diffLines) console.log(`  ${l}`);
      console.log("  ...");
    }
    console.log();
  }

  void sql;
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
