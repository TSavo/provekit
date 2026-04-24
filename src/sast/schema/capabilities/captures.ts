import {
  sqliteTable,
  text,
  integer,
  index,
  primaryKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

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

export function registerCaptures(): void {
  registerCapability({
    dslName: "captures",
    table: nodeCaptures,
    columns: {
      node_id:         { dslName: "node_id",         drizzleColumn: nodeCaptures.nodeId,         isNodeRef: true,  nullable: false },
      captured_name:   { dslName: "captured_name",   drizzleColumn: nodeCaptures.capturedName,   sort: "Text",     isNodeRef: false, nullable: false },
      declared_in_node: { dslName: "declared_in_node", drizzleColumn: nodeCaptures.declaredInNode, isNodeRef: true, nullable: true },
      mutable:         { dslName: "mutable",         drizzleColumn: nodeCaptures.mutable,         sort: "Bool",     isNodeRef: false, nullable: false },
    },
  });
}

registerCaptures();
