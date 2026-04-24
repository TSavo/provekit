import { sqliteTable, text, index } from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeArithmetic = sqliteTable(
  "node_arithmetic",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    op: text("op").notNull(),
    lhsNode: text("lhs_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    rhsNode: text("rhs_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    resultSort: text("result_sort").notNull(),
  },
  (t) => ({
    byOpNodeId: index("node_arithmetic_by_op_node_id").on(t.op, t.nodeId),
  }),
);
