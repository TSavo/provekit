import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeDecides = sqliteTable(
  "node_decides",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    conditionNode: text("condition_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    consequentNode: text("consequent_node").references(() => nodes.id, { onDelete: "cascade" }),
    alternateNode: text("alternate_node").references(() => nodes.id, { onDelete: "cascade" }),
    decisionKind: text("decision_kind").notNull(),
  },
);
