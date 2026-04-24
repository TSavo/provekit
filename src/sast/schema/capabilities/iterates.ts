import {
  sqliteTable,
  text,
  integer,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeIterates = sqliteTable(
  "node_iterates",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    initNode: text("init_node").references(() => nodes.id, { onDelete: "cascade" }),
    conditionNode: text("condition_node").references(() => nodes.id, { onDelete: "cascade" }),
    updateNode: text("update_node").references(() => nodes.id, { onDelete: "cascade" }),
    bodyNode: text("body_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    loopKind: text("loop_kind").notNull(),
    executesAtLeastOnce: integer("executes_at_least_once", { mode: "boolean" }).notNull(),
    collectionSourceNode: text("collection_source_node").references(() => nodes.id, { onDelete: "cascade" }),
  },
);
