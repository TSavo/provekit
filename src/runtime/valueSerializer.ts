import type { Db } from "../db/index.js";
import { runtimeValues, runtimeValueObjectMembers, runtimeValueArrayElements } from "../db/schema/index.js";

const MAX_STRING_BYTES = 1024;
const MAX_ARRAY_ELEMENTS = 100;
const MAX_OBJECT_KEYS = 100;

export function serializeValue(db: Db, value: unknown): number {
  const seen = new Map<object, number>();
  return writeValue(db, value, seen);
}

function writeValue(db: Db, v: unknown, seen: Map<object, number>): number {
  if (v === null) return insert(db, { kind: "null" });
  if (v === undefined) return insert(db, { kind: "undefined" });

  const t = typeof v;

  if (t === "boolean") return insert(db, { kind: "bool", boolValue: v as boolean });

  if (t === "number") {
    const n = v as number;
    if (Number.isNaN(n)) return insert(db, { kind: "nan" });
    if (n === Infinity) return insert(db, { kind: "infinity" });
    if (n === -Infinity) return insert(db, { kind: "neg_infinity" });
    return insert(db, { kind: "number", numberValue: n });
  }

  if (t === "string") {
    const s = v as string;
    if (s.length > MAX_STRING_BYTES) {
      return insert(db, { kind: "truncated", truncationNote: `string of length ${s.length} truncated` });
    }
    return insert(db, { kind: "string", stringValue: s });
  }

  if (t === "bigint") return insert(db, { kind: "bigint", stringValue: (v as bigint).toString() });
  if (t === "symbol") return insert(db, { kind: "symbol", stringValue: String(v) });
  if (t === "function") {
    return insert(db, { kind: "function", stringValue: (v as Function).name || "<anonymous>" });
  }

  // object or array
  const obj = v as object;
  if (seen.has(obj)) {
    return insert(db, { kind: "circular", circularTargetId: seen.get(obj)! });
  }

  if (Array.isArray(obj)) {
    const id = insert(db, { kind: "array" });
    seen.set(obj, id);
    const len = Math.min(obj.length, MAX_ARRAY_ELEMENTS);
    for (let i = 0; i < len; i++) {
      const childId = writeValue(db, (obj as any[])[i], seen);
      db.insert(runtimeValueArrayElements).values({
        parentValueId: id,
        elementIndex: i,
        childValueId: childId,
      }).run();
    }
    if (obj.length > MAX_ARRAY_ELEMENTS) {
      const truncId = insert(db, { kind: "truncated", truncationNote: `array of length ${obj.length} truncated at ${MAX_ARRAY_ELEMENTS}` });
      db.insert(runtimeValueArrayElements).values({
        parentValueId: id,
        elementIndex: MAX_ARRAY_ELEMENTS,
        childValueId: truncId,
      }).run();
    }
    return id;
  }

  // plain object
  const id = insert(db, { kind: "object" });
  seen.set(obj, id);
  const keys = Object.keys(obj).slice(0, MAX_OBJECT_KEYS);
  for (const k of keys) {
    const childId = writeValue(db, (obj as Record<string, unknown>)[k], seen);
    db.insert(runtimeValueObjectMembers).values({
      parentValueId: id,
      key: k,
      childValueId: childId,
    }).run();
  }
  return id;
}

function insert(db: Db, values: {
  kind: "number" | "string" | "bool" | "null" | "undefined" | "object" | "array" | "function" | "bigint" | "symbol" | "nan" | "infinity" | "neg_infinity" | "circular" | "truncated";
  numberValue?: number;
  stringValue?: string;
  boolValue?: boolean;
  circularTargetId?: number;
  truncationNote?: string;
}): number {
  const row = db.insert(runtimeValues).values(values).returning().get();
  return row.id;
}
