import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";

export const nodeNonNullAssertion = sqliteTable(
  "node_non_null_assertion",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    operandNode: text("operand_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
);
