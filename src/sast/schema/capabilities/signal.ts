import {
  sqliteTable,
  text,
  integer,
  index,
  primaryKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

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

export function registerSignal(): void {
  registerCapability({
    dslName: "signal",
    table: nodeSignal,
    columns: {
      node_id:        { dslName: "node_id",        drizzleColumn: nodeSignal.nodeId,        isNodeRef: true,  nullable: false },
      signal_kind:    { dslName: "signal_kind",    drizzleColumn: nodeSignal.signalKind,    sort: "Text",     isNodeRef: false, nullable: false,
                        kindEnum: ["log", "type_annotation", "throw_message", "todo_comment", "error_message"] },
      signal_payload: { dslName: "signal_payload", drizzleColumn: nodeSignal.signalPayload, sort: "Text",     isNodeRef: false, nullable: false },
    },
  });
}

export function registerSignalInterpolations(): void {
  registerCapability({
    dslName: "signal_interpolations",
    table: signalInterpolations,
    columns: {
      signal_node:       { dslName: "signal_node",       drizzleColumn: signalInterpolations.signalNode,       isNodeRef: true,  nullable: false },
      slot_index:        { dslName: "slot_index",        drizzleColumn: signalInterpolations.slotIndex,        sort: "Int",      isNodeRef: false, nullable: false },
      interpolated_node: { dslName: "interpolated_node", drizzleColumn: signalInterpolations.interpolatedNode, isNodeRef: true,  nullable: false },
    },
  });
}

registerSignal();
registerSignalInterpolations();
