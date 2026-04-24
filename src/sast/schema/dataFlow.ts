import {
  sqliteTable,
  text,
  index,
  primaryKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "./nodes.js";

export const dataFlow = sqliteTable(
  "data_flow",
  {
    toNode: text("to_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    fromNode: text("from_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    slot: text("slot").notNull(),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.toNode, t.fromNode, t.slot] }),
    byToNodeSlot: index("data_flow_by_to_node_slot_from_node").on(t.toNode, t.slot, t.fromNode),
    byFromNode: index("data_flow_by_from_node_to_node").on(t.fromNode, t.toNode),
  }),
);

export const dataFlowTransitive = sqliteTable(
  "data_flow_transitive",
  {
    toNode: text("to_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    fromNode: text("from_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.toNode, t.fromNode] }),
    byFromNode: index("data_flow_transitive_by_from_node_to_node").on(t.fromNode, t.toNode),
  }),
);
