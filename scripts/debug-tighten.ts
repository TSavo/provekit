#!/usr/bin/env tsx
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../src/db/index.js";
import { buildSASTForFile } from "../src/sast/builder.js";
import { evaluatePrinciple } from "../src/dsl/evaluator.js";

const scratchDir = mkdtempSync(join(tmpdir(), "tighten-test-"));
mkdirSync(join(scratchDir, "src"), { recursive: true });
const filePath = join(scratchDir, "src", "guarded.ts");
writeFileSync(filePath, `function f(n: number): number {
  if (n < 100) {
    return n + 1;
  }
  return 0;
}
`, "utf-8");

const db = openDb(join(scratchDir, "scratch.db"));
migrate(db, { migrationsFolder: "drizzle" });
buildSASTForFile(db, filePath);

console.log("=== node_narrows rows ===");
const narrowRows = db.$client.prepare(`SELECT n.kind, narrows.* FROM node_narrows narrows JOIN nodes n ON n.id = narrows.node_id`).all();
for (const r of narrowRows) console.log(r);

console.log("\n=== node_arithmetic rows ===");
const arithRows = db.$client.prepare(`SELECT n.kind, arith.* FROM node_arithmetic arith JOIN nodes n ON n.id = arith.node_id`).all();
for (const r of arithRows) console.log(r);

console.log("\n=== data_flow rows (first 10) ===");
const dfRows = db.$client.prepare(`SELECT * FROM data_flow LIMIT 10`).all();
for (const r of dfRows) console.log(r);

const addDsl = readFileSync(".provekit/principles/addition-overflow.dsl", "utf-8");
console.log("\n=== running addition-overflow on guarded code (n<100; return n+1) ===");
console.log("Expected: principle SUPPRESSED (no match) because n is bounded");
try {
  evaluatePrinciple(db, addDsl);
} catch (e: any) {
  console.log("evaluatePrinciple error:", e.message);
}
const matchRows = db.$client.prepare(`SELECT * FROM principle_matches`).all();
console.log("matches found:", matchRows.length);
for (const r of matchRows) console.log(r);

rmSync(scratchDir, { recursive: true, force: true });
db.$client.close();
