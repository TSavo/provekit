#!/usr/bin/env tsx
/**
 * Reproduce the UNIQUE constraint failed: nodes.id error in pass 10.
 *
 * Sequence:
 *   1. Open overlay rooted at promptlib HEAD (planted bug present in HEAD)
 *   2. Pre-index a "wrong locus" file (reference/toolstac/...): like Investigate
 *      mistakenly identified
 *   3. Apply a patch to a DIFFERENT file (the actual buggy repositories.ts)
 *   4. reindexOverlay runs reindexFile on the modified file
 *   5. Observe the UNIQUE failure (or not, if it's specific to that locus)
 */
import { join } from "path";
import { mkdtempSync, rmSync, readFileSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { fileURLToPath } from "url";
import { dirname } from "path";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import { openDb } from "../src/db/index.js";
import { buildSASTForFile, reindexFile } from "../src/sast/builder.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "drizzle");

const PROMPTLIB = "/Users/tsavo/projects/promptlib";
const scratch = mkdtempSync(join(tmpdir(), "overlay-repro-"));
console.log("scratch:", scratch);

// Mimic openOverlay: git worktree add at HEAD
execFileSync("git", ["worktree", "add", "--detach", scratch, "HEAD"], { cwd: PROMPTLIB });

const dbPath = join(scratch, ".provekit", "scratch-sast.db");
execFileSync("mkdir", ["-p", join(scratch, ".provekit")]);
const db = openDb(dbPath);
migrate(db, { migrationsFolder });

console.log("\n=== Step 1: pre-index 'wrong locus' file ===");
const wrongLocus = join(scratch, "reference/toolstac/core/prompt-evolution-service.ts");
buildSASTForFile(db, wrongLocus);
console.log("OK: indexed", wrongLocus);

console.log("\n=== Step 2: edit a different file (the real bug) ===");
const realFile = join(scratch, "src/store/sqlite/repositories.ts");
const before = readFileSync(realFile, "utf-8");
const patched = before.replace(".orderBy(asc(", ".orderBy(desc(");
writeFileSync(realFile, patched);
console.log("OK: patched", realFile);

// Inspect the DB state between steps
import { files, nodes } from "../src/sast/schema/index.js";
const filesBefore = db.select().from(files).all();
console.log("\nfiles table after step 1:", filesBefore.map((r: any) => `id=${r.id} path=${r.path.slice(-40)}`));
const nodesCountBefore = db.select().from(nodes).all().length;
console.log("nodes count after step 1:", nodesCountBefore);

console.log("\n=== Step 3: reindexFile on the patched file ===");
try {
  reindexFile(db, realFile);
  console.log("OK: reindex succeeded");
} catch (err) {
  console.error("FAIL:", (err as Error).message);
  console.error("Stack:", (err as Error).stack?.split("\n").slice(0, 8).join("\n"));
  // Inspect post-failure state
  const filesAfter = db.select().from(files).all();
  console.error("files table after FAIL:", filesAfter.map((r: any) => `id=${r.id} path=${r.path.slice(-40)}`));
  // Find a duplicated node ID
  const nodeIds = db.select({ id: nodes.id, fileId: nodes.fileId }).from(nodes).all();
  const idCounts: Record<string, Array<number>> = {};
  for (const n of nodeIds) {
    if (!idCounts[n.id]) idCounts[n.id] = [];
    idCounts[n.id]!.push(n.fileId);
  }
  const dupes = Object.entries(idCounts).filter(([, fids]) => fids.length > 1);
  console.error("duplicated node IDs:", dupes.slice(0, 3));
}

// Cleanup
try { execFileSync("git", ["worktree", "remove", "--force", scratch], { cwd: PROMPTLIB }); } catch {}
try { rmSync(scratch, { recursive: true, force: true }); } catch {}
