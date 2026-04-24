import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeTruthiness = sqliteTable(
  "node_truthiness",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    coercionKind: text("coercion_kind").notNull(),
    operandNode: text("operand_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    byCoercionKindNodeId: index("node_truthiness_by_coercion_kind_node_id").on(t.coercionKind, t.nodeId),
  }),
);
