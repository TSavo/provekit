import { sqliteTable, integer, text, index, uniqueIndex, primaryKey } from "drizzle-orm/sqlite-core";
import { sql } from "drizzle-orm";
import { runtimeValues } from "./runtimeValues.js";

// NB: Phase A-thin keeps node_id as a TEXT source reference.
// Phase B will add FK to nodes(id) once SAST is built.

export const traces = sqliteTable(
  "traces",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    clauseId: integer("clause_id").notNull(),
    capturedAt: integer("captured_at").notNull(),
    outcomeKind: text("outcome_kind", {
      enum: ["returned", "threw", "untestable"],
    }).notNull(),
    outcomeValueId: integer("outcome_value_id").references(() => runtimeValues.id),
    untestableReason: text("untestable_reason"),
    inputsHash: text("inputs_hash").notNull(),
  },
  (t) => ({
    byClause: index("traces_by_clause").on(t.clauseId),
  }),
);

export const traceValues = sqliteTable(
  "trace_values",
  {
    traceId: integer("trace_id")
      .notNull()
      .references(() => traces.id, { onDelete: "cascade" }),
    nodeId: text("node_id").notNull(),
    iterationIndex: integer("iteration_index"),
    rootValueId: integer("root_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.traceId, t.nodeId, t.iterationIndex] }),
    byNode: index("tv_by_node").on(t.nodeId),
    byRoot: index("tv_by_root").on(t.rootValueId),
    singlePointUnique: uniqueIndex("tv_single_point_unique")
      .on(t.traceId, t.nodeId)
      .where(sql`iteration_index IS NULL`),
  }),
);
