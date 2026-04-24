import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeNarrows = sqliteTable(
  "node_narrows",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    targetNode: text("target_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    narrowingKind: text("narrowing_kind").notNull(),
    narrowedType: text("narrowed_type"),
  },
  (t) => ({
    byNarrowingKindNodeId: index("node_narrows_by_narrowing_kind_node_id").on(t.narrowingKind, t.nodeId),
  }),
);

export function registerNarrows(): void {
  registerCapability({
    dslName: "narrows",
    table: nodeNarrows,
    columns: {
      node_id:       { dslName: "node_id",       drizzleColumn: nodeNarrows.nodeId,       isNodeRef: true,  nullable: false },
      target_node:   { dslName: "target_node",   drizzleColumn: nodeNarrows.targetNode,   isNodeRef: true,  nullable: false },
      narrowing_kind: { dslName: "narrowing_kind", drizzleColumn: nodeNarrows.narrowingKind, sort: "Text",  isNodeRef: false, nullable: false,
                        kindEnum: ["typeof", "instanceof", "in", "null_check", "undefined_check", "literal_eq", "tag_check"] },
      narrowed_type: { dslName: "narrowed_type", drizzleColumn: nodeNarrows.narrowedType, sort: "Text",     isNodeRef: false, nullable: true },
    },
  });
}

registerNarrows();
