import {
  sqliteTable,
  text,
  index,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeTruthiness = sqliteTable(
  "node_truthiness",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    coercionKind: text("coercion_kind").notNull(),
    operandNode: text("operand_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    byCoercionKindNodeId: index("node_truthiness_by_coercion_kind_node_id").on(t.coercionKind, t.nodeId),
  }),
);

export function registerTruthiness(): void {
  registerCapability({
    dslName: "truthiness",
    table: nodeTruthiness,
    columns: {
      node_id:      { dslName: "node_id",      drizzleColumn: nodeTruthiness.nodeId,      isNodeRef: true,  nullable: false },
      coercion_kind: { dslName: "coercion_kind", drizzleColumn: nodeTruthiness.coercionKind, sort: "Text",  isNodeRef: false, nullable: false,
                       kindEnum: ["truthy_test", "falsy_default", "nullish_coalesce", "strict_eq_null"] },
      operand_node: { dslName: "operand_node", drizzleColumn: nodeTruthiness.operandNode, isNodeRef: true,  nullable: false },
    },
  });
}

registerTruthiness();
