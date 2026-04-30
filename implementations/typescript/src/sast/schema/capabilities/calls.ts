import {
  sqliteTable,
  text,
  integer,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

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

export function registerCalls(): void {
  registerCapability({
    dslName: "calls",
    table: nodeCalls,
    columns: {
      node_id:        { dslName: "node_id",        drizzleColumn: nodeCalls.nodeId,        isNodeRef: true,  nullable: false },
      callee_node:    { dslName: "callee_node",    drizzleColumn: nodeCalls.calleeNode,    isNodeRef: true,  nullable: false },
      callee_name:    { dslName: "callee_name",    drizzleColumn: nodeCalls.calleeName,    sort: "Text",     isNodeRef: false, nullable: true },
      arg_count:      { dslName: "arg_count",      drizzleColumn: nodeCalls.argCount,      sort: "Int",      isNodeRef: false, nullable: false },
      is_method_call: { dslName: "is_method_call", drizzleColumn: nodeCalls.isMethodCall,  sort: "Bool",     isNodeRef: false, nullable: false },
      callee_is_async: { dslName: "callee_is_async", drizzleColumn: nodeCalls.calleeIsAsync, sort: "Bool",   isNodeRef: false, nullable: false },
    },
  });
}

registerCalls();
