import {
  sqliteTable,
  text,
  integer,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeIterates = sqliteTable(
  "node_iterates",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    initNode: text("init_node").references(() => nodes.id, { onDelete: "cascade" }),
    conditionNode: text("condition_node").references(() => nodes.id, { onDelete: "cascade" }),
    updateNode: text("update_node").references(() => nodes.id, { onDelete: "cascade" }),
    bodyNode: text("body_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    loopKind: text("loop_kind").notNull(),
    executesAtLeastOnce: integer("executes_at_least_once", { mode: "boolean" }).notNull(),
    collectionSourceNode: text("collection_source_node").references(() => nodes.id, { onDelete: "cascade" }),
  },
);

export function registerIterates(): void {
  registerCapability({
    dslName: "iterates",
    table: nodeIterates,
    columns: {
      node_id:               { dslName: "node_id",               drizzleColumn: nodeIterates.nodeId,               isNodeRef: true,  nullable: false },
      init_node:             { dslName: "init_node",             drizzleColumn: nodeIterates.initNode,             isNodeRef: true,  nullable: true },
      condition_node:        { dslName: "condition_node",        drizzleColumn: nodeIterates.conditionNode,        isNodeRef: true,  nullable: true },
      update_node:           { dslName: "update_node",           drizzleColumn: nodeIterates.updateNode,           isNodeRef: true,  nullable: true },
      body_node:             { dslName: "body_node",             drizzleColumn: nodeIterates.bodyNode,             isNodeRef: true,  nullable: false },
      loop_kind:             { dslName: "loop_kind",             drizzleColumn: nodeIterates.loopKind,             sort: "Text",     isNodeRef: false, nullable: false,
                               kindEnum: ["for", "while", "do_while", "for_of", "for_in"] },
      executes_at_least_once: { dslName: "executes_at_least_once", drizzleColumn: nodeIterates.executesAtLeastOnce, sort: "Bool",   isNodeRef: false, nullable: false },
      collection_source_node: { dslName: "collection_source_node", drizzleColumn: nodeIterates.collectionSourceNode, isNodeRef: true, nullable: true },
    },
  });
}

registerIterates();
