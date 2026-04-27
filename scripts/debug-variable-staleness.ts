#!/usr/bin/env tsx
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../src/db/index.js";
import { buildSASTForFile } from "../src/sast/builder.js";
import { evaluatePrinciple } from "../src/dsl/evaluator.js";

const scratchDir = mkdtempSync(join(tmpdir(), "var-staleness-test-"));
mkdirSync(join(scratchDir, "src"), { recursive: true });

const buggyPath = join(scratchDir, "src", "buggy.ts");
writeFileSync(buggyPath, `export function applyDiscount(hasCoupon: boolean, total: number): number {
  let discount = 0;
  if (hasCoupon) {
    discount = 10;
  }
  return total - discount;
}
`, "utf-8");

const db = openDb(join(scratchDir, "scratch.db"));
migrate(db, { migrationsFolder: "drizzle" });
buildSASTForFile(db, buggyPath);

console.log("=== node_decides rows ===");
const decRows = db.$client.prepare(`SELECT n.kind, n.source_line, d.* FROM node_decides d JOIN nodes n ON n.id = d.node_id`).all();
for (const r of decRows) console.log(r);

console.log("\n=== node_assigns rows (assign_kind = '=') ===");
const asRows = db.$client.prepare(`SELECT n.kind, n.source_line, a.* FROM node_assigns a JOIN nodes n ON n.id = a.node_id WHERE a.assign_kind = '='`).all();
for (const r of asRows) console.log(r);

console.log("\n=== data_flow_transitive rows ===");
const dfRows = db.$client.prepare(`SELECT count(*) AS c FROM data_flow_transitive`).all();
console.log(dfRows);

console.log("\n=== Running variable-staleness ===");
try {
  evaluatePrinciple(db, readFileSync(".provekit/principles/variable-staleness.dsl", "utf-8"));
} catch (e: any) {
  console.log("evaluatePrinciple error:", e.message);
}

const matchRows = db.$client.prepare(
  `SELECT pm.principle_name, n.source_line FROM principle_matches pm JOIN nodes n ON n.id = pm.root_match_node_id`,
).all() as Array<{ principle_name: string; source_line: number }>;
console.log(`matches: ${matchRows.length}`);
for (const r of matchRows) console.log(`  line ${r.source_line}  ${r.principle_name}`);

rmSync(scratchDir, { recursive: true, force: true });
db.$client.close();
