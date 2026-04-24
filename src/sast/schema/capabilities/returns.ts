import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeReturns = sqliteTable(
  "node_returns",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    exitKind: text("exit_kind").notNull(),
    valueNode: text("value_node").references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    byExitKindNodeId: index("node_returns_by_exit_kind_node_id").on(t.exitKind, t.nodeId),
  }),
);

export function registerReturns(): void {
  registerCapability({
    dslName: "returns",
    table: nodeReturns,
    columns: {
      node_id:   { dslName: "node_id",   drizzleColumn: nodeReturns.nodeId,   isNodeRef: true,  nullable: false },
      exit_kind: { dslName: "exit_kind", drizzleColumn: nodeReturns.exitKind, sort: "Text",     isNodeRef: false, nullable: false,
                   kindEnum: ["return", "throw", "process_exit"] },
      value_node: { dslName: "value_node", drizzleColumn: nodeReturns.valueNode, isNodeRef: true, nullable: true },
    },
  });
}

registerReturns();
