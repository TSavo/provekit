import {
  sqliteTable,
  text,
  integer,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeThrows = sqliteTable(
  "node_throws",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    handlerNode: text("handler_node").references(() => nodes.id, { onDelete: "cascade" }),
    isInsideHandler: integer("is_inside_handler", { mode: "boolean" }).notNull(),
  },
);

export function registerThrows(): void {
  registerCapability({
    dslName: "throws",
    table: nodeThrows,
    columns: {
      node_id:          { dslName: "node_id",          drizzleColumn: nodeThrows.nodeId,          isNodeRef: true,  nullable: false },
      handler_node:     { dslName: "handler_node",     drizzleColumn: nodeThrows.handlerNode,     isNodeRef: true,  nullable: true },
      is_inside_handler: { dslName: "is_inside_handler", drizzleColumn: nodeThrows.isInsideHandler, sort: "Bool",  isNodeRef: false, nullable: false },
    },
  });
}

registerThrows();
