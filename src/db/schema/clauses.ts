import { sqliteTable, integer, text, index, primaryKey, foreignKey } from "drizzle-orm/sqlite-core";
import { runtimeValues } from "./runtimeValues.js";

// Phase A-thin: contractKey is a free-form text reference to the v1 JSON contract
// (format: file/fn[line], matching existing signalKey). Phase A-full will migrate
// this to FK against a contracts table.

export const clauses = sqliteTable(
  "clauses",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    contractKey: text("contract_key").notNull(),
    verdict: text("verdict", {
      enum: ["proven", "violation", "unknown", "vacuous"],
    }).notNull(),
    smt2: text("smt2").notNull(),
    clauseHash: text("clause_hash").notNull(),
    principleName: text("principle_name"),
    complexity: integer("complexity"),
    confidence: text("confidence"),
    judgeNote: text("judge_note"),
    vacuousReason: text("vacuous_reason"),
  },
  (t) => ({
    byContract: index("clauses_by_contract").on(t.contractKey),
    byHash: index("clauses_by_hash").on(t.clauseHash),
  }),
);

export const clauseBindings = sqliteTable(
  "clause_bindings",
  {
    clauseId: integer("clause_id")
      .notNull()
      .references(() => clauses.id, { onDelete: "cascade" }),
    smtConstant: text("smt_constant").notNull(),
    // Phase A-thin: line-referenced. Phase B adds `boundToNode` FK to nodes.
    sourceLine: integer("source_line").notNull(),
    sourceExpr: text("source_expr").notNull(),
    sort: text("sort").notNull(),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.clauseId, t.smtConstant] }),
  }),
);

export const clauseWitnesses = sqliteTable(
  "clause_witnesses",
  {
    clauseId: integer("clause_id").notNull(),
    smtConstant: text("smt_constant").notNull(),
    modelValueId: integer("model_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.clauseId, t.smtConstant] }),
    byValue: index("cw_by_value").on(t.modelValueId),
    bindingFk: foreignKey({
      columns: [t.clauseId, t.smtConstant],
      foreignColumns: [clauseBindings.clauseId, clauseBindings.smtConstant],
    }).onDelete("cascade"),
  }),
);
