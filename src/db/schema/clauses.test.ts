import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues } from "./runtimeValues.js";
import { clauses, clauseBindings, clauseWitnesses } from "./clauses.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("clauses schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a clause with a binding and a witness referencing a runtime_value", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db
      .insert(clauses)
      .values({
        contractKey: "src/math.ts/divide[10]",
        verdict: "violation",
        smt2: "(declare-const den Real) (assert (= den 0))",
        clauseHash: "abc",
      })
      .returning()
      .get();

    db.insert(clauseBindings)
      .values({
        clauseId: clause.id,
        smtConstant: "den",
        sourceLine: 10,
        sourceExpr: "denominator",
        sort: "Real",
      })
      .run();

    const wvalue = db
      .insert(runtimeValues)
      .values({ kind: "number", numberValue: 0 })
      .returning()
      .get();

    db.insert(clauseWitnesses)
      .values({
        clauseId: clause.id,
        smtConstant: "den",
        modelValueId: wvalue.id,
        sort: "Real",
      })
      .run();

    const bindings = db.select().from(clauseBindings).where(eq(clauseBindings.clauseId, clause.id)).all();
    expect(bindings).toHaveLength(1);
    expect(bindings[0].sourceLine).toBe(10);

    const witnesses = db
      .select({ constant: clauseWitnesses.smtConstant, value: runtimeValues.numberValue })
      .from(clauseWitnesses)
      .innerJoin(runtimeValues, eq(runtimeValues.id, clauseWitnesses.modelValueId))
      .where(eq(clauseWitnesses.clauseId, clause.id))
      .all();
    expect(witnesses).toEqual([{ constant: "den", value: 0 }]);
  });
});
