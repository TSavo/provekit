import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeNonNullAssertion = sqliteTable(
  "node_non_null_assertion",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    operandNode: text("operand_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
);

export function registerNonNullAssertion(): void {
  registerCapability({
    dslName: "non_null_assertion",
    table: nodeNonNullAssertion,
    columns: {
      node_id:     { dslName: "node_id",     drizzleColumn: nodeNonNullAssertion.nodeId,     isNodeRef: true, nullable: false },
      operand_node: { dslName: "operand_node", drizzleColumn: nodeNonNullAssertion.operandNode, isNodeRef: true, nullable: false },
    },
  });
}

registerNonNullAssertion();
