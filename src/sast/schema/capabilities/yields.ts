import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeYields = sqliteTable(
  "node_yields",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    yieldKind: text("yield_kind").notNull(),
    sourceCallNode: text("source_call_node").references(() => nodes.id, { onDelete: "cascade" }),
  },
);

export function registerYields(): void {
  registerCapability({
    dslName: "yields",
    table: nodeYields,
    columns: {
      node_id:         { dslName: "node_id",         drizzleColumn: nodeYields.nodeId,         isNodeRef: true,  nullable: false },
      yield_kind:      { dslName: "yield_kind",      drizzleColumn: nodeYields.yieldKind,      sort: "Text",     isNodeRef: false, nullable: false,
                         kindEnum: ["await", "yield"] },
      source_call_node: { dslName: "source_call_node", drizzleColumn: nodeYields.sourceCallNode, isNodeRef: true, nullable: true },
    },
  });
}

registerYields();
