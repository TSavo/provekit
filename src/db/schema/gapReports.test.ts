import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues } from "./runtimeValues.js";
import { clauses } from "./clauses.js";
import { gapReports } from "./gapReports.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("gap_reports schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a gap_report referencing a clause + two runtime_values", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/math.ts/divide[10]",
      verdict: "proven",
      smt2: "(assert (not (= den 0)))",
      clauseHash: "c1",
    }).returning().get();

    const smtVal = db.insert(runtimeValues).values({ kind: "number", numberValue: 0 }).returning().get();
    const rtVal = db.insert(runtimeValues).values({ kind: "nan" }).returning().get();

    const gap = db.insert(gapReports).values({
      clauseId: clause.id,
      kind: "ieee_specials",
      smtConstant: "den",
      smtValueId: smtVal.id,
      runtimeValueId: rtVal.id,
      explanation: "SMT Real does not model IEEE NaN",
    }).returning().get();

    const rows = db.select().from(gapReports).where(eq(gapReports.id, gap.id)).all();
    expect(rows).toHaveLength(1);
    expect(rows[0].kind).toBe("ieee_specials");
    expect(rows[0].smtValueId).toBe(smtVal.id);
    expect(rows[0].runtimeValueId).toBe(rtVal.id);

    db.$client.close();
  });
});
