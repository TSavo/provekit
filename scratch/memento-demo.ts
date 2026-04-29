/**
 * Empirical demo of the memento store's cache benefit.
 *
 * Sets up a tmp project with a synthetic invariant, runs verifyAll
 * twice against the same memento DB, and reports timing + memento
 * stats. The first run populates the table (cache miss); the second
 * run hits the cache (engine skipped).
 *
 * Run: npx tsx scratch/memento-demo.ts
 */

import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { createHash } from "crypto";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../src/db/index.js";
import { writeInvariant, type StoredInvariant } from "../src/fix/runtime/invariantStore.js";
import { verifyAll } from "../src/fix/runtime/verify.js";
import { stats } from "../src/fix/runtime/mementoStore.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "drizzle");

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

async function main() {
  const root = mkdtempSync(join(tmpdir(), "memento-demo-"));
  mkdirSync(join(root, "src"), { recursive: true });
  mkdirSync(join(root, ".provekit"), { recursive: true });

  // Memento DB
  const mementoDb = openDb(join(root, ".provekit", "memento.db"));
  migrate(mementoDb, { migrationsFolder: DRIZZLE_FOLDER });

  // Plant a small file and a synthetic invariant against it
  const filePath = "src/example.ts";
  const content = "function divide(a: number, b: number) {\n  return a / b;\n}\n";
  writeFileSync(join(root, filePath), content);
  const span = content.split("\n").slice(0, 3).join("\n");

  const inv: StoredInvariant = {
    id: "demo-inv-1",
    createdAt: new Date().toISOString(),
    originatingBug: "memento-demo",
    smt: {
      kind: "arithmetic",
      declarations: ["(declare-const b Int)"],
      assertion: "(assert (not (= b 0)))",
    },
    bindings: [
      {
        type: "local",
        smt_constant: "b",
        source_expr: "b",
        sort: "Int",
        node: {
          filePath,
          nodeHash: hash16(span),
          startLine: 1,
          endLine: 3,
        },
      },
    ],
    callsite: { filePath, function: "divide", startLine: 2, endLine: 2 },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
  writeInvariant(root, inv);

  console.log(`project: ${root}`);
  console.log();

  // Run 1: cache miss
  const t0 = Date.now();
  const r1 = await verifyAll(root, { mementoDb });
  const t1 = Date.now();
  console.log(`Run 1 (cache miss):`);
  console.log(`  duration:      ${t1 - t0}ms`);
  console.log(`  verdict:       ${r1.verdicts[0].status}`);
  console.log(`  cached?:       ${r1.verdicts[0].note?.includes("cached") ? "YES" : "no"}`);
  console.log(`  pathCheck:     ${r1.verdicts[0].pathCheck}`);
  console.log();

  const s1 = stats(mementoDb);
  console.log(`Memento store after run 1: ${s1.totalRows} row(s), ${s1.uniqueKeys} unique key(s)`);
  console.log();

  // Run 2: cache hit
  const t2 = Date.now();
  const r2 = await verifyAll(root, { mementoDb });
  const t3 = Date.now();
  console.log(`Run 2 (cache hit):`);
  console.log(`  duration:      ${t3 - t2}ms`);
  console.log(`  verdict:       ${r2.verdicts[0].status}`);
  console.log(`  cached?:       ${r2.verdicts[0].note?.includes("cached") ? "YES" : "no"}`);
  console.log(`  pathCheck:     ${r2.verdicts[0].pathCheck}`);
  console.log(`  note:          ${r2.verdicts[0].note}`);
  console.log();

  console.log(`Speedup (cache hit / cache miss): ${(t1 - t0) / Math.max(t3 - t2, 1)}x`);
}

main().catch((err) => {
  console.error("demo failed:", err);
  process.exit(1);
});
