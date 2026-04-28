#!/usr/bin/env tsx
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../src/db/index.js";
import { buildSASTForFile } from "../src/sast/builder.js";
import { evaluatePrinciple } from "../src/dsl/evaluator.js";

const scratchDir = mkdtempSync(join(tmpdir(), "loop-accum-test-"));
mkdirSync(join(scratchDir, "src"), { recursive: true });

const buggyPath = join(scratchDir, "src", "buggy.ts");
writeFileSync(buggyPath, `function sumToN(n: number): number {
  let total = 0;
  for (let i = 0; i < n; i++) {
    total += i;
  }
  return total;
}
`, "utf-8");

const cleanPath = join(scratchDir, "src", "clean.ts");
writeFileSync(cleanPath, `function sumToN(n: number): number {
  let total = 0;
  total += n * (n - 1) / 2;
  return total;
}
`, "utf-8");

const db = openDb(join(scratchDir, "scratch.db"));
migrate(db, { migrationsFolder: "drizzle" });
buildSASTForFile(db, buggyPath);
buildSASTForFile(db, cleanPath);

console.log("=== iterates rows ===");
const itRows = db.$client.prepare(`SELECT n.kind, n.source_line, it.* FROM node_iterates it JOIN nodes n ON n.id = it.node_id`).all();
for (const r of itRows) console.log(r);

console.log("\n=== assigns +=  rows ===");
const asRows = db.$client.prepare(`SELECT n.kind, n.source_line, a.* FROM node_assigns a JOIN nodes n ON n.id = a.node_id WHERE a.assign_kind = '+='`).all();
for (const r of asRows) console.log(r);

console.log("\n=== running loop-accumulator-overflow ===");
console.log("Expected: 1 match (buggy.ts line 4); 0 matches on clean.ts (no for-loop)");
try {
  evaluatePrinciple(db, readFileSync(".provekit/principles/universal/loop-accumulator-overflow.dsl", "utf-8"));
} catch (e: any) {
  console.log("evaluatePrinciple error:", e.message);
}
const matchRows = db.$client.prepare(`SELECT pm.*, n.source_line, f.path FROM principle_matches pm JOIN nodes n ON n.id = pm.root_match_node_id JOIN files f ON f.id = pm.file_id`).all();
console.log("matches:", matchRows.length);
for (const r of matchRows) console.log(r);

rmSync(scratchDir, { recursive: true, force: true });
db.$client.close();
