import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeReturns = sqliteTable(
  "node_returns",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    exitKind: text("exit_kind").notNull(),
    valueNode: text("value_node").references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    byExitKindNodeId: index("node_returns_by_exit_kind_node_id").on(t.exitKind, t.nodeId),
  }),
);
