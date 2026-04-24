import {
  sqliteTable,
  text,
  integer,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeCalls = sqliteTable(
  "node_calls",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    calleeNode: text("callee_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    calleeName: text("callee_name"),
    argCount: integer("arg_count").notNull(),
    isMethodCall: integer("is_method_call", { mode: "boolean" }).notNull(),
    calleeIsAsync: integer("callee_is_async", { mode: "boolean" }).notNull(),
  },
  (t) => ({
    byCalleeNameNodeId: index("node_calls_by_callee_name_node_id").on(t.calleeName, t.nodeId),
  }),
);
