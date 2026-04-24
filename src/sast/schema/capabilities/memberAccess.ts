import {
  sqliteTable,
  text,
  integer,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeMemberAccess = sqliteTable(
  "node_member_access",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    objectNode: text("object_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    propertyName: text("property_name"),
    computed: integer("computed", { mode: "boolean" }).notNull(),
  },
  (t) => ({
    byPropertyNameObjectNode: index("node_member_access_by_property_name_object_node").on(t.propertyName, t.objectNode),
  }),
);

export function registerMemberAccess(): void {
  registerCapability({
    dslName: "member_access",
    table: nodeMemberAccess,
    columns: {
      node_id:       { dslName: "node_id",       drizzleColumn: nodeMemberAccess.nodeId,       isNodeRef: true,  nullable: false },
      object_node:   { dslName: "object_node",   drizzleColumn: nodeMemberAccess.objectNode,   isNodeRef: true,  nullable: false },
      property_name: { dslName: "property_name", drizzleColumn: nodeMemberAccess.propertyName, sort: "Text",     isNodeRef: false, nullable: true },
      computed:      { dslName: "computed",       drizzleColumn: nodeMemberAccess.computed,     sort: "Bool",     isNodeRef: false, nullable: false },
    },
  });
}

registerMemberAccess();
