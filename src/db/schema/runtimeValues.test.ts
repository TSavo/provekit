import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues, runtimeValueObjectMembers } from "./runtimeValues.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("runtime_values schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a primitive number value and reads it back", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const inserted = db
      .insert(runtimeValues)
      .values({ kind: "number", numberValue: 42 })
      .returning()
      .get();

    const back = db
      .select()
      .from(runtimeValues)
      .where(eq(runtimeValues.id, inserted.id))
      .get();

    expect(back?.kind).toBe("number");
    expect(back?.numberValue).toBe(42);
  });

  it("inserts an object with a member and joins through the edge table", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const obj = db.insert(runtimeValues).values({ kind: "object" }).returning().get();
    const ok = db.insert(runtimeValues).values({ kind: "bool", boolValue: false }).returning().get();
    db.insert(runtimeValueObjectMembers).values({ parentValueId: obj.id, key: "ok", childValueId: ok.id }).run();

    const rows = db
      .select({ key: runtimeValueObjectMembers.key, childKind: runtimeValues.kind, childBool: runtimeValues.boolValue })
      .from(runtimeValueObjectMembers)
      .innerJoin(runtimeValues, eq(runtimeValues.id, runtimeValueObjectMembers.childValueId))
      .where(eq(runtimeValueObjectMembers.parentValueId, obj.id))
      .all();

    expect(rows).toEqual([{ key: "ok", childKind: "bool", childBool: false }]);
  });
});
