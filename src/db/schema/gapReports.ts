import { sqliteTable, integer, text, index } from "drizzle-orm/sqlite-core";
import { clauses } from "./clauses.js";
import { traces } from "./traces.js";
import { runtimeValues } from "./runtimeValues.js";

export const gapReports = sqliteTable(
  "gap_reports",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    clauseId: integer("clause_id")
      .notNull()
      .references(() => clauses.id, { onDelete: "cascade" }),
    traceId: integer("trace_id").references(() => traces.id),
    kind: text("kind", {
      enum: [
        "ieee_specials",
        "int_overflow",
        "bool_coercion",
        "null_undefined",
        "path_not_taken",
        "outcome_mismatch",
        "invalid_binding",
      ],
    }).notNull(),
    smtConstant: text("smt_constant"),
    // Phase A-thin: node reference as free-form text; Phase B adds FK.
    atNodeRef: text("at_node_ref"),
    smtValueId: integer("smt_value_id").references(() => runtimeValues.id),
    runtimeValueId: integer("runtime_value_id").references(() => runtimeValues.id),
    explanation: text("explanation"),
  },
  (t) => ({
    byClause: index("gr_by_clause").on(t.clauseId),
    byKind: index("gr_by_kind").on(t.kind),
    byNodeRef: index("gr_by_node_ref").on(t.atNodeRef),
  }),
);
