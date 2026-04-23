import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../db/index.js";
import { runtimeValues, runtimeValueObjectMembers, runtimeValueArrayElements } from "../db/schema/index.js";
import { serializeValue } from "./valueSerializer.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("serializeValue", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  function freshDb() {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });
    return db;
  }

  it("serializes a primitive number", () => {
    const db = freshDb();
    const id = serializeValue(db, 42);
    const row = db.select().from(runtimeValues).where(eq(runtimeValues.id, id)).get();
    expect(row?.kind).toBe("number");
    expect(row?.numberValue).toBe(42);
  });

  it("serializes NaN as kind='nan'", () => {
    const db = freshDb();
    const id = serializeValue(db, NaN);
    const row = db.select().from(runtimeValues).where(eq(runtimeValues.id, id)).get();
    expect(row?.kind).toBe("nan");
  });

  it("serializes +Infinity and -Infinity", () => {
    const db = freshDb();
    const pos = serializeValue(db, Infinity);
    const neg = serializeValue(db, -Infinity);
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, pos)).get()?.kind).toBe("infinity");
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, neg)).get()?.kind).toBe("neg_infinity");
  });

  it("serializes null and undefined", () => {
    const db = freshDb();
    const nullId = serializeValue(db, null);
    const undefId = serializeValue(db, undefined);
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, nullId)).get()?.kind).toBe("null");
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, undefId)).get()?.kind).toBe("undefined");
  });

  it("serializes a flat object", () => {
    const db = freshDb();
    const id = serializeValue(db, { name: "x", count: 3 });
    const members = db.select().from(runtimeValueObjectMembers).where(eq(runtimeValueObjectMembers.parentValueId, id)).all();
    expect(members).toHaveLength(2);
    const byKey = new Map(members.map((m) => [m.key, m.childValueId]));
    const nameChild = db.select().from(runtimeValues).where(eq(runtimeValues.id, byKey.get("name")!)).get();
    expect(nameChild?.kind).toBe("string");
    expect(nameChild?.stringValue).toBe("x");
  });

  it("serializes an array", () => {
    const db = freshDb();
    const id = serializeValue(db, [10, 20, 30]);
    const rows = db.select().from(runtimeValueArrayElements).where(eq(runtimeValueArrayElements.parentValueId, id)).all();
    expect(rows).toHaveLength(3);
    const vals = rows.sort((a, b) => a.elementIndex - b.elementIndex).map((r) => {
      return db.select().from(runtimeValues).where(eq(runtimeValues.id, r.childValueId)).get()?.numberValue;
    });
    expect(vals).toEqual([10, 20, 30]);
  });

  it("serializes a circular reference with kind='circular'", () => {
    const db = freshDb();
    const o: any = { name: "root" };
    o.self = o;
    const id = serializeValue(db, o);
    const members = db.select().from(runtimeValueObjectMembers).where(eq(runtimeValueObjectMembers.parentValueId, id)).all();
    const selfMember = members.find((m) => m.key === "self");
    expect(selfMember).toBeDefined();
    const child = db.select().from(runtimeValues).where(eq(runtimeValues.id, selfMember!.childValueId)).get();
    expect(child?.kind).toBe("circular");
    expect(child?.circularTargetId).toBe(id);
  });

  it("truncates long strings", () => {
    const db = freshDb();
    const longStr = "x".repeat(2000);
    const id = serializeValue(db, longStr);
    const row = db.select().from(runtimeValues).where(eq(runtimeValues.id, id)).get();
    expect(row?.kind).toBe("truncated");
  });
});
