import { sqliteTable, text, index } from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeAssigns = sqliteTable(
  "node_assigns",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    targetNode: text("target_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    rhsNode: text("rhs_node").references(() => nodes.id, { onDelete: "cascade" }),
    assignKind: text("assign_kind").notNull(),
  },
  (t) => ({
    byAssignKindNodeId: index("node_assigns_by_assign_kind_node_id").on(t.assignKind, t.nodeId),
  }),
);

export function registerAssigns(): void {
  registerCapability({
    dslName: "assigns",
    table: nodeAssigns,
    columns: {
      node_id:     { dslName: "node_id",     drizzleColumn: nodeAssigns.nodeId,     isNodeRef: true,  nullable: false },
      target_node: { dslName: "target_node", drizzleColumn: nodeAssigns.targetNode, isNodeRef: true,  nullable: false },
      rhs_node:    { dslName: "rhs_node",    drizzleColumn: nodeAssigns.rhsNode,    isNodeRef: true,  nullable: true },
      assign_kind: { dslName: "assign_kind", drizzleColumn: nodeAssigns.assignKind, sort: "Text",     isNodeRef: false, nullable: false,
                     kindEnum: ["=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", ">>=", "&=", "|=", "^=", "&&=", "||=", "??=", "delete"] },
    },
  });
}

registerAssigns();
