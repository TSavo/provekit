import {
  sqliteTable,
  text,
  integer,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

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
