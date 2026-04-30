import type { Db } from "../db/index.js";
import { clauseWitnesses, runtimeValues } from "../db/schema/index.js";
import type { Z3Value } from "./modelParser.js";

export function persistWitness(db: Db, clauseId: number, model: Map<string, Z3Value>): void {
  for (const [constantName, z3val] of model) {
    const valueRow = writeZ3Value(db, z3val);
    db.insert(clauseWitnesses).values({
      clauseId,
      smtConstant: constantName,
      modelValueId: valueRow.id,
    }).run();
  }
}

function writeZ3Value(db: Db, v: Z3Value): { id: number } {
  if (v.sort === "Real") {
    if (typeof v.value === "number") {
      return db.insert(runtimeValues).values({ kind: "number", numberValue: v.value }).returning().get();
    }
    if (v.value === "div_by_zero" || v.value === "nan") {
      return db.insert(runtimeValues).values({ kind: "nan" }).returning().get();
    }
    if (v.value === "+infinity") return db.insert(runtimeValues).values({ kind: "infinity" }).returning().get();
    if (v.value === "-infinity") return db.insert(runtimeValues).values({ kind: "neg_infinity" }).returning().get();
  }
  if (v.sort === "Int") {
    const n = Number(v.value);
    if (Number.isSafeInteger(n)) {
      return db.insert(runtimeValues).values({ kind: "number", numberValue: n }).returning().get();
    }
    return db.insert(runtimeValues).values({ kind: "bigint", stringValue: v.value.toString() }).returning().get();
  }
  if (v.sort === "Bool") {
    return db.insert(runtimeValues).values({ kind: "bool", boolValue: v.value }).returning().get();
  }
  if (v.sort === "String") {
    return db.insert(runtimeValues).values({ kind: "string", stringValue: v.value }).returning().get();
  }
  // Other / unrecognized: store raw as a string
  const raw = "raw" in v ? v.raw : JSON.stringify(v);
  return db.insert(runtimeValues).values({ kind: "string", stringValue: raw }).returning().get();
}
