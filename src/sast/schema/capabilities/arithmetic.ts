import { sqliteTable, text, index } from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodeArithmetic = sqliteTable(
  "node_arithmetic",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    op: text("op").notNull(),
    lhsNode: text("lhs_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    rhsNode: text("rhs_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    resultSort: text("result_sort").notNull(),
  },
  (t) => ({
    byOpNodeId: index("node_arithmetic_by_op_node_id").on(t.op, t.nodeId),
  }),
);

export function registerArithmetic(): void {
  registerCapability({
    dslName: "arithmetic",
    table: nodeArithmetic,
    columns: {
      node_id:     { dslName: "node_id",     drizzleColumn: nodeArithmetic.nodeId,     isNodeRef: true,  nullable: false },
      op:          { dslName: "op",          drizzleColumn: nodeArithmetic.op,          sort: "Text",     isNodeRef: false, nullable: false,
                     kindEnum: ["+", "-", "*", "/", "%", "**", "<<", ">>", ">>>", "&", "|", "^"] },
      lhs_node:    { dslName: "lhs_node",    drizzleColumn: nodeArithmetic.lhsNode,    isNodeRef: true,  nullable: false },
      rhs_node:    { dslName: "rhs_node",    drizzleColumn: nodeArithmetic.rhsNode,    isNodeRef: true,  nullable: false },
      result_sort: { dslName: "result_sort", drizzleColumn: nodeArithmetic.resultSort, sort: "Text",     isNodeRef: false, nullable: false },
    },
  });
}

registerArithmetic();
