import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeNarrows = sqliteTable(
  "node_narrows",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    targetNode: text("target_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    narrowingKind: text("narrowing_kind").notNull(),
    narrowedType: text("narrowed_type"),
  },
  (t) => ({
    byNarrowingKindNodeId: index("node_narrows_by_narrowing_kind_node_id").on(t.narrowingKind, t.nodeId),
  }),
);
