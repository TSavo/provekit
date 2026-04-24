import {
  sqliteTable,
  text,
  integer,
  index,
  primaryKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeSignal = sqliteTable(
  "node_signal",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    signalKind: text("signal_kind").notNull(),
    signalPayload: text("signal_payload").notNull(),
  },
  (t) => ({
    bySignalKindNodeId: index("node_signal_by_signal_kind_node_id").on(t.signalKind, t.nodeId),
  }),
);

export const signalInterpolations = sqliteTable(
  "signal_interpolations",
  {
    signalNode: text("signal_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    slotIndex: integer("slot_index").notNull(),
    interpolatedNode: text("interpolated_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.signalNode, t.slotIndex] }),
    byInterpolatedNode: index("signal_interpolations_by_interpolated_node").on(t.interpolatedNode),
  }),
);
