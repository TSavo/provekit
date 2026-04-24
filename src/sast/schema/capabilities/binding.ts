import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

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
