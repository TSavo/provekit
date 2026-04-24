import {
  sqliteTable,
  text,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../nodes.js";
import { registerCapability } from "../../capabilityRegistry.js";

export const nodePattern = sqliteTable(
  "node_pattern",
  {
    nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),
    patternKind: text("pattern_kind").notNull(),
    slotKey: text("slot_key"),
    renameTo: text("rename_to"),
    defaultSmt: text("default_smt"),
  },
);

export function registerPattern(): void {
  registerCapability({
    dslName: "pattern",
    table: nodePattern,
    columns: {
      node_id:      { dslName: "node_id",      drizzleColumn: nodePattern.nodeId,      isNodeRef: true,  nullable: false },
      pattern_kind: { dslName: "pattern_kind", drizzleColumn: nodePattern.patternKind, sort: "Text",     isNodeRef: false, nullable: false,
                      kindEnum: ["object", "array", "rest", "identifier"] },
      slot_key:     { dslName: "slot_key",     drizzleColumn: nodePattern.slotKey,     sort: "Text",     isNodeRef: false, nullable: true },
      rename_to:    { dslName: "rename_to",    drizzleColumn: nodePattern.renameTo,    sort: "Text",     isNodeRef: false, nullable: true },
      default_smt:  { dslName: "default_smt",  drizzleColumn: nodePattern.defaultSmt,  sort: "Text",     isNodeRef: false, nullable: true },
    },
  });
}

registerPattern();
