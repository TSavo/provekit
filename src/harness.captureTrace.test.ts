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

  it("runs two harnesses concurrently without snapshot cross-contamination", async () => {
    // Regression test for the global-state race: before the unique-per-call
    // snapshot fn name was introduced, two parallel runHarnessWithTrace calls
    // clobbered each other's capture arrays via globalThis.__neurallog_snapshot__.
    // Each run here returns a distinct value and should see its own captures.
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    mkdirSync(join(tmpDir, "src"));

    const pathA = join(tmpDir, "src", "addA.ts");
    const pathB = join(tmpDir, "src", "addB.ts");
    writeFileSync(pathA, `export function addA(x: number): number {\n  const sum = x + 100;\n  return sum;\n}`);
    writeFileSync(pathB, `export function addB(x: number): number {\n  const sum = x + 200;\n  return sum;\n}`);

    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });
    const clauseA = db.insert(clauses).values({ contractKey: "a", verdict: "proven", smt2: "(assert true)", clauseHash: "a" }).returning().get();
    const clauseB = db.insert(clauses).values({ contractKey: "b", verdict: "proven", smt2: "(assert true)", clauseHash: "b" }).returning().get();

    const [resA, resB] = await Promise.all([
      runHarnessWithTrace({
        db, clauseId: clauseA.id, sourcePath: pathA, functionName: "addA",
        signalLine: 3, captureNames: ["x", "sum"], inputs: { x: 1 },
      }),
      runHarnessWithTrace({
        db, clauseId: clauseB.id, sourcePath: pathB, functionName: "addB",
        signalLine: 3, captureNames: ["x", "sum"], inputs: { x: 1 },
      }),
    ]);

    // addA(1) returns 101, addB(1) returns 201. If globals raced, either both
    // come back with the same 'sum' (one run wrote into the other's array) or
    // one has zero captures (the delete in finally wiped the wrong slot).
    const readSum = (traceId: number) => {
      const rows = db
        .select({ nodeId: traceValues.nodeId, num: runtimeValues.numberValue })
        .from(traceValues)
        .innerJoin(runtimeValues, eq(runtimeValues.id, traceValues.rootValueId))
        .where(eq(traceValues.traceId, traceId))
        .all();
      const sum = rows.find(r => r.nodeId.endsWith(":sum"));
      return sum?.num;
    };

    expect(resA.outcomeKind).toBe("returned");
    expect(resB.outcomeKind).toBe("returned");
    expect(readSum(resA.traceId)).toBe(101);
    expect(readSum(resB.traceId)).toBe(201);

    db.$client.close();
  });
});
