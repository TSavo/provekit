import { sqliteTable, text, index } from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeAssigns = sqliteTable(
  "node_assigns",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    targetNode: text("target_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    rhsNode: text("rhs_node").references(() => nodes.id, { onDelete: "cascade" }),
    assignKind: text("assign_kind").notNull(),
  },
  (t) => ({
    byAssignKindNodeId: index("node_assigns_by_assign_kind_node_id").on(t.assignKind, t.nodeId),
  }),
);
