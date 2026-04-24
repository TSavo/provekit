import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodePattern = sqliteTable(
  "node_pattern",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    patternKind: text("pattern_kind").notNull(),
    slotKey: text("slot_key"),
    renameTo: text("rename_to"),
    defaultSmt: text("default_smt"),
  },
);
