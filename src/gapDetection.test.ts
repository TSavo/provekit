import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { clauses, clauseBindings, gapReports, runtimeValues } from "./db/schema/index.js";
import { detectGaps } from "./gapDetection.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("detectGaps", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("detects IEEE NaN gap when SMT models finite result but runtime produces NaN", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    mkdirSync(join(tmpDir, "src"));
    const srcPath = join(tmpDir, "src", "divide.ts");
    writeFileSync(srcPath, `
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("q", q);
  return q;
}
    `.trim());

    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/divide.ts/divide[2]",
      verdict: "proven",
      smt2: "(declare-const q Real) (assert (= q 0.0))",
      clauseHash: "c1",
    }).returning().get();

    await detectGaps({
      db,
      clauseId: clause.id,
      sourcePath: srcPath,
      functionName: "divide",
      signalLine: 3,
      bindings: [
        { smtConstant: "q", sourceLine: 2, sourceExpr: "a / b", sort: "Real" },
      ],
      z3WitnessText: `(
        (define-fun q () Real 0.0)
      )`,
      inputs: { a: 0, b: 0 },
    });

    const gaps = db.select().from(gapReports).where(eq(gapReports.clauseId, clause.id)).all();
    expect(gaps.length).toBeGreaterThanOrEqual(1);
    const ieeeGap = gaps.find((g) => g.kind === "ieee_specials");
    expect(ieeeGap).toBeDefined();
    expect(ieeeGap!.explanation).toMatch(/NaN/);

    db.$client.close();
  });
});
