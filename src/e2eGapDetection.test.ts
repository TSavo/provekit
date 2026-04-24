import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, copyFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { clauses, gapReports } from "./db/schema/index.js";
import { detectGaps } from "./gapDetection.js";
import { explainGaps } from "./cli.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

// End-to-end: drive the full Phase D pipeline against the committed
// examples/division-by-zero.ts fixture and assert the IEEE gap reaches the
// explain output via the real runtime_values graph.
//
// This ties all prior pieces together: validator -> Z3 parser -> harness
// instrumentation -> value serializer -> comparator agents -> gap_reports
// insert -> explainGaps render.

describe("e2e: division-by-zero fixture produces IEEE gap end-to-end", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("runs the full pipeline and reports an IEEE NaN gap via explainGaps", async () => {
    // Copy the committed fixture into a scratch dir so the instrumented
    // sibling file doesn't pollute the repo.
    tmpDir = mkdtempSync(join(tmpdir(), "provekit-e2e-"));
    mkdirSync(join(tmpDir, "src"));
    const srcPath = join(tmpDir, "src", "divide.ts");
    copyFileSync(join(__dirname, "..", "examples", "division-by-zero.ts"), srcPath);

    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const contractKey = "src/divide.ts/divide[2]";
    const clause = db.insert(clauses).values({
      contractKey,
      verdict: "proven",
      smt2: "(declare-const q Real) (assert (= q 0.0))",
      clauseHash: "c1",
    }).returning().get();

    // Z3's model asserting the quotient is the finite value 0.0 — the
    // shape a naive Real encoding of `a / b` produces when the LLM misses
    // that JavaScript division is IEEE, not algebraic Real.
    const witnessText = `(
      (define-fun q () Real 0.0)
    )`;

    // Line layout in examples/division-by-zero.ts:
    //   1-4: header comments   5: blank    6: export function divide
    //   7:   const q = a / b;  8: console.log("q", q);  9: return q;  10: }
    // Signal is the console.log on line 8; q is computed on line 7.
    await detectGaps({
      db,
      clauseId: clause.id,
      sourcePath: srcPath,
      functionName: "divide",
      signalLine: 8,
      bindings: [
        { smtConstant: "q", sourceLine: 7, sourceExpr: "a / b", sort: "Real" },
      ],
      z3WitnessText: witnessText,
      inputs: { a: 0, b: 0 }, // 0/0 is the canonical NaN case
    });

    // At least one IEEE gap is recorded against this clause.
    const gaps = db.select().from(gapReports).where(eq(gapReports.clauseId, clause.id)).all();
    const ieeeGaps = gaps.filter((g) => g.kind === "ieee_specials");
    expect(ieeeGaps.length).toBeGreaterThanOrEqual(1);

    // The gap round-trips through explainGaps to the rendered output.
    const output = explainGaps(db, contractKey);
    expect(output).toContain("encoding-gap");
    expect(output).toContain("— q");
    expect(output).toContain("NaN"); // runtime side
    expect(output).toContain("ieee_specials");

    db.$client.close();
  });
});
