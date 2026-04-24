import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

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

export function registerDecides(): void {
  registerCapability({
    dslName: "decides",
    table: nodeDecides,
    columns: {
      node_id:        { dslName: "node_id",        drizzleColumn: nodeDecides.nodeId,        isNodeRef: true,  nullable: false },
      condition_node: { dslName: "condition_node", drizzleColumn: nodeDecides.conditionNode, isNodeRef: true,  nullable: false },
      consequent_node: { dslName: "consequent_node", drizzleColumn: nodeDecides.consequentNode, isNodeRef: true, nullable: true },
      alternate_node: { dslName: "alternate_node", drizzleColumn: nodeDecides.alternateNode, isNodeRef: true,  nullable: true },
      decision_kind:  { dslName: "decision_kind",  drizzleColumn: nodeDecides.decisionKind,  sort: "Text",     isNodeRef: false, nullable: false,
                        kindEnum: ["if", "ternary", "short_circuit_and", "short_circuit_or", "nullish", "optional_chain", "switch_case"] },
    },
  });
}

registerDecides();
