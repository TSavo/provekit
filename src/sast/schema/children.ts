import {
  sqliteTable,
  integer,
  text,
  index,
  primaryKey,
  foreignKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "./nodes.js";

export const nodeChildren = sqliteTable(
  "node_children",
  {
    parentId: text("parent_id").notNull(),
    childId: text("child_id").notNull(),
    childOrder: integer("child_order").notNull(),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.parentId, t.childId] }),
    byChildId: index("nc_by_child_id").on(t.childId),
    parentFk: foreignKey({
      columns: [t.parentId],
      foreignColumns: [nodes.id],
    }).onDelete("cascade"),
    childFk: foreignKey({
      columns: [t.childId],
      foreignColumns: [nodes.id],
    }).onDelete("cascade"),
  }),
);
