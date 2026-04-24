import {
  sqliteTable,
  text,
  integer,
  index,
  primaryKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeCaptures = sqliteTable(
  "node_captures",
  {
    nodeId: text("node_id").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    capturedName: text("captured_name").notNull(),
    declaredInNode: text("declared_in_node").references(() => nodes.id, { onDelete: "cascade" }),
    mutable: integer("mutable", { mode: "boolean" }).notNull(),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.nodeId, t.capturedName] }),
    byCapturedName: index("node_captures_by_captured_name").on(t.capturedName),
  }),
);
