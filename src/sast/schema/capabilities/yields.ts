import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeYields = sqliteTable(
  "node_yields",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    yieldKind: text("yield_kind").notNull(),
    sourceCallNode: text("source_call_node").references(() => nodes.id, { onDelete: "cascade" }),
  },
);
