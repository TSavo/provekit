import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeBinding = sqliteTable(
  "node_binding",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    name: text("name").notNull(),
    declaredType: text("declared_type"),
    bindingKind: text("binding_kind").notNull(),
  },
  (t) => ({
    byName: index("node_binding_by_name").on(t.name),
    byBindingKindNodeId: index("node_binding_by_binding_kind_node_id").on(t.bindingKind, t.nodeId),
  }),
);

export function registerBinding(): void {
  registerCapability({
    dslName: "binding",
    table: nodeBinding,
    columns: {
      node_id:      { dslName: "node_id",      drizzleColumn: nodeBinding.nodeId,      isNodeRef: true,  nullable: false },
      name:         { dslName: "name",         drizzleColumn: nodeBinding.name,         sort: "Text",     isNodeRef: false, nullable: false },
      declared_type: { dslName: "declared_type", drizzleColumn: nodeBinding.declaredType, sort: "Text",  isNodeRef: false, nullable: true },
      binding_kind: { dslName: "binding_kind", drizzleColumn: nodeBinding.bindingKind,  sort: "Text",     isNodeRef: false, nullable: false,
                      kindEnum: ["const", "let", "var", "param", "function", "class"] },
    },
  });
}

registerBinding();
