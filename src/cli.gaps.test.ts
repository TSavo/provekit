import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { clauses, gapReports, runtimeValues } from "./db/schema/index.js";
import { explainGaps } from "./cli.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

describe("explainGaps", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("renders gap reports for a given contract key with Z3 vs runtime values", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db
      .insert(clauses)
      .values({
        contractKey: "src/divide.ts/divide[2]",
        verdict: "proven",
        smt2: "(assert (= q 0.0))",
        clauseHash: "c1",
      })
      .returning()
      .get();

    const smtVal = db.insert(runtimeValues).values({ kind: "number", numberValue: 0 }).returning().get();
    const rtVal = db.insert(runtimeValues).values({ kind: "nan" }).returning().get();

    db.insert(gapReports).values({
      clauseId: clause.id,
      kind: "ieee_specials",
      smtConstant: "q",
      atNodeRef: "src/divide.ts:2",
      smtValueId: smtVal.id,
      runtimeValueId: rtVal.id,
      explanation: "SMT Real modeled 0 but runtime produced NaN",
    }).run();

    const output = explainGaps(db, "src/divide.ts/divide[2]");

    // Header names the location and the SMT constant
    expect(output).toContain("encoding-gap at src/divide.ts:2");
    expect(output).toContain("— q");
    // Renders both sides of the comparison
    expect(output).toMatch(/Z3 modeled:\s+0/);
    expect(output).toMatch(/Runtime returned:\s+NaN/);
    // Includes the cause and kind
    expect(output).toContain("SMT Real modeled 0 but runtime produced NaN");
    expect(output).toContain("ieee_specials");

    db.$client.close();
  });

  it("reports no gaps when none are stored for the given contract", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const output = explainGaps(db, "src/nope.ts/foo[1]");
    expect(output).toMatch(/no encoding gaps/i);

    db.$client.close();
  });

  it("handles a gap with no smt/runtime values (e.g., invalid_binding)", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/x.ts/fn[1]",
      verdict: "violation",
      smt2: "(assert true)",
      clauseHash: "h",
    }).returning().get();

    db.insert(gapReports).values({
      clauseId: clause.id,
      kind: "invalid_binding",
      smtConstant: "ghost",
      explanation: "source_expr 'phantom' not found at line 2",
    }).run();

    const output = explainGaps(db, "src/x.ts/fn[1]");
    expect(output).toContain("encoding-gap");
    expect(output).toContain("— ghost");
    expect(output).toContain("invalid_binding");
    expect(output).toContain("source_expr 'phantom' not found");
    // No Z3-modeled / Runtime-returned lines when neither value id is set
    expect(output).not.toMatch(/Z3 modeled:/);
    expect(output).not.toMatch(/Runtime returned:/);

    db.$client.close();
  });
});
