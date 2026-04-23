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
});
