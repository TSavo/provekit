import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../db/index.js";
import { clauses, clauseBindings, clauseWitnesses, runtimeValues } from "../db/schema/index.js";
import { persistWitness } from "./persistWitness.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("persistWitness", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("writes parsed witness values to clause_witnesses and runtime_values", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "provekit-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/math.ts/divide[10]",
      verdict: "violation",
      smt2: "(assert (= den 0))",
      clauseHash: "c1",
    }).returning().get();

    // Bindings must exist before witnesses (composite FK).
    for (const [name, sort] of [["den", "Real"], ["count", "Int"], ["ok", "Bool"]] as const) {
      db.insert(clauseBindings).values({
        clauseId: clause.id,
        smtConstant: name,
        sourceLine: 10,
        sourceExpr: name,
        sort,
      }).run();
    }

    persistWitness(db, clause.id, new Map([
      ["den", { sort: "Real", value: 0 }],
      ["count", { sort: "Int", value: 5n }],
      ["ok", { sort: "Bool", value: false }],
    ]));

    const witnesses = db.select().from(clauseWitnesses).where(eq(clauseWitnesses.clauseId, clause.id)).all();
    expect(witnesses).toHaveLength(3);

    const byName = new Map(witnesses.map((w) => [w.smtConstant, w]));
    const denVal = db.select().from(runtimeValues).where(eq(runtimeValues.id, byName.get("den")!.modelValueId)).get();
    expect(denVal?.kind).toBe("number");
    expect(denVal?.numberValue).toBe(0);

    const okVal = db.select().from(runtimeValues).where(eq(runtimeValues.id, byName.get("ok")!.modelValueId)).get();
    expect(okVal?.kind).toBe("bool");
    expect(okVal?.boolValue).toBe(false);

    db.$client.close();
  });
});
