#!/usr/bin/env tsx
/**
 * End-to-end harvest-time diff write — exercises the full path from a
 * synthetic HarvestCandidate through computeFileDiff to pre_post_diff
 * rows in a scratch DB. Then queries back the row counts to verify the
 * DSL-relation lookup pattern works (find by post coordinates).
 */
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { fileURLToPath } from "url";
import { dirname } from "path";
import { eq, and } from "drizzle-orm";

import { openDb } from "../src/db/index.js";
import { prePostDiff } from "../src/db/schema/preDiff.js";
import { recordCandidateDiff, clearCandidateDiff } from "../src/fix/harvest/diff.js";
import type { HarvestCandidate } from "../src/fix/harvest/extractBugs.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const scratchDir = mkdtempSync(join(tmpdir(), "provekit-diff-smoke-"));
const dbPath = join(scratchDir, "test.db");
const db = openDb(dbPath);
migrate(db, { migrationsFolder: join(__dirname, "..", "drizzle") });

const candidate: HarvestCandidate = {
  source: {
    project: "synthetic",
    bugId: "1",
    baseSha: "deadbeef",
    fixSha: "f1xc0de",
    testSha: null,
    originalSha: null,
  },
  buggyFiles: {
    "src/divide.ts": `function divide(a: number, b: number): number {
  return a / b;
}`,
    "src/check.ts": `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`,
  },
  fixedFiles: {
    "src/divide.ts": `function divide(a: number, b: number): number {
  if (b === 0) throw new Error("Division by zero");
  return a / b;
}`,
    "src/check.ts": `function check(t: string): boolean {
  return t === "Foo" || t === "Bar" || t === "Baz";
}`,
  },
  diff: "",
  upstreamFixMessage: "",
  testFiles: {},
  stats: { filesChanged: 2, insertions: 2, deletions: 0 },
};

clearCandidateDiff(db, candidate.source.project, candidate.source.bugId);
const result = recordCandidateDiff(db, candidate);
console.log(`Files processed: ${result.filesProcessed}`);
console.log(`Rows inserted:   ${result.rowsInserted}`);
for (const f of result.perFile) {
  console.log(`  ${f.filePath}: ${JSON.stringify(f.summary)}`);
}

// DSL-style lookup: find all "added" nodes for the divide.ts file.
const context = `harvest:${candidate.source.project}:${candidate.source.bugId}`;
const addedInDivide = db
  .select()
  .from(prePostDiff)
  .where(
    and(
      eq(prePostDiff.context, context),
      eq(prePostDiff.filePath, "src/divide.ts"),
      eq(prePostDiff.changeKind, "added"),
    ),
  )
  .all();
console.log();
console.log(`'added' nodes in src/divide.ts: ${addedInDivide.length}`);
const ifAdded = addedInDivide.find((r) => r.postKind === "IfStatement");
if (ifAdded) {
  console.log(`  ✓ IfStatement marked added: ${ifAdded.postTextPreview}`);
} else {
  console.log(`  ✗ no IfStatement marked added`);
  process.exit(1);
}

const addedInCheck = db
  .select()
  .from(prePostDiff)
  .where(
    and(
      eq(prePostDiff.context, context),
      eq(prePostDiff.filePath, "src/check.ts"),
      eq(prePostDiff.changeKind, "added"),
    ),
  )
  .all();
console.log(`'added' nodes in src/check.ts: ${addedInCheck.length}`);
const bazAdded = addedInCheck.find(
  (r) => r.postKind === "BinaryExpression" && /Baz/.test(r.postTextPreview ?? ""),
);
if (bazAdded) {
  console.log(`  ✓ Baz BinaryExpression marked added: ${bazAdded.postTextPreview}`);
} else {
  console.log(`  ✗ no BinaryExpression containing Baz marked added`);
  process.exit(1);
}

// Idempotent re-run: clear, re-write, count again.
const cleared = clearCandidateDiff(db, candidate.source.project, candidate.source.bugId);
console.log();
console.log(`Cleared ${cleared} rows. Re-running...`);
const second = recordCandidateDiff(db, candidate);
if (second.rowsInserted !== result.rowsInserted) {
  console.log(`  ✗ row count drift: ${result.rowsInserted} → ${second.rowsInserted}`);
  process.exit(1);
}
console.log(`  ✓ idempotent: ${second.rowsInserted} rows`);

rmSync(scratchDir, { recursive: true, force: true });
console.log();
console.log(`PASS — pre_post_diff harvest write end-to-end`);
