import {
  sqliteTable,
  text,
  integer,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeThrows = sqliteTable(
  "node_throws",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    handlerNode: text("handler_node").references(() => nodes.id, { onDelete: "cascade" }),
    isInsideHandler: integer("is_inside_handler", { mode: "boolean" }).notNull(),
  },
);
