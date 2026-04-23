import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { traces, traceValues, clauses, runtimeValues } from "./db/schema/index.js";
import { runHarnessWithTrace } from "./harness.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("runHarnessWithTrace", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("executes the target function, captures locals at signal line, persists as trace + trace_values", async () => {
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
      verdict: "violation",
      smt2: "(assert (not (= b 0)))",
      clauseHash: "c1",
    }).returning().get();

    const result = await runHarnessWithTrace({
      db,
      clauseId: clause.id,
      sourcePath: srcPath,
      functionName: "divide",
      signalLine: 3,
      captureNames: ["a", "b", "q"],
      inputs: { a: 1, b: 0 },
    });

    expect(result.outcomeKind).toBe("returned");

    const traceRows = db.select().from(traces).where(eq(traces.clauseId, clause.id)).all();
    expect(traceRows).toHaveLength(1);

    const valRows = db
      .select({ nodeId: traceValues.nodeId, kind: runtimeValues.kind, num: runtimeValues.numberValue })
      .from(traceValues)
      .innerJoin(runtimeValues, eq(runtimeValues.id, traceValues.rootValueId))
      .where(eq(traceValues.traceId, traceRows[0].id))
      .all();

    expect(valRows.length).toBeGreaterThanOrEqual(3);
    const byNode = new Map(valRows.map((r) => [r.nodeId, r]));
    const qRow = [...byNode.entries()].find(([k]) => k.includes("q"));
    expect(qRow?.[1].kind === "infinity" || qRow?.[1].kind === "nan").toBeTruthy();

    db.$client.close();
  });
});
