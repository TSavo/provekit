import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues } from "./runtimeValues.js";
import { traces, traceValues } from "./traces.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("traces schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a trace and a trace_value referencing a runtime_value", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const ret = db.insert(runtimeValues).values({ kind: "number", numberValue: 42 }).returning().get();
    const trace = db
      .insert(traces)
      .values({
        clauseId: 1,
        capturedAt: Date.now(),
        outcomeKind: "returned",
        outcomeValueId: ret.id,
        inputsHash: "abc123",
      })
      .returning()
      .get();

    const snap = db.insert(runtimeValues).values({ kind: "number", numberValue: 5 }).returning().get();
    db.insert(traceValues).values({
      traceId: trace.id,
      nodeId: "src/foo.ts:10:5",
      iterationIndex: null,
      rootValueId: snap.id,
    }).run();

    const rows = db
      .select({ kind: runtimeValues.kind, value: runtimeValues.numberValue })
      .from(traceValues)
      .innerJoin(runtimeValues, eq(runtimeValues.id, traceValues.rootValueId))
      .where(eq(traceValues.traceId, trace.id))
      .all();
    expect(rows).toEqual([{ kind: "number", value: 5 }]);
  });

  it("tv_single_point_unique: rejects duplicate (traceId, nodeId, null) but allows distinct iterationIndex", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    // Seed a runtime_value and a trace to reference
    const rv = db.insert(runtimeValues).values({ kind: "number", numberValue: 1 }).returning().get();
    const trace = db
      .insert(traces)
      .values({
        clauseId: 1,
        capturedAt: Date.now(),
        outcomeKind: "returned",
        inputsHash: "hashA",
      })
      .returning()
      .get();

    const nodeId = "src/foo.ts:10:5";

    // First insert with null iterationIndex — must succeed
    db.insert(traceValues).values({ traceId: trace.id, nodeId, iterationIndex: null, rootValueId: rv.id }).run();

    // Second insert with same (traceId, nodeId, null) — must throw UNIQUE constraint
    expect(() =>
      db.insert(traceValues).values({ traceId: trace.id, nodeId, iterationIndex: null, rootValueId: rv.id }).run(),
    ).toThrow(/UNIQUE constraint failed/);

    // Inserts with non-null iterationIndex on the same (traceId, nodeId) — must succeed
    // (partial index only fires for NULL; composite PK handles uniqueness for non-null rows)
    db.insert(traceValues).values({ traceId: trace.id, nodeId, iterationIndex: 0, rootValueId: rv.id }).run();
    db.insert(traceValues).values({ traceId: trace.id, nodeId, iterationIndex: 1, rootValueId: rv.id }).run();
  });
});
