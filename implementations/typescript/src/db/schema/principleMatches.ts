import {
  sqliteTable,
  integer,
  text,
  index,
  foreignKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "../../sast/schema/nodes.js";

export const principleMatches = sqliteTable(
  "principle_matches",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    principleName: text("principle_name").notNull(),
    fileId: integer("file_id").notNull(),
    rootMatchNodeId: text("root_match_node_id").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    severity: text("severity", { enum: ["violation", "warning", "info"] }).notNull(),
    message: text("message").notNull(),
  },
  (t) => ({
    byPrincipleName: index("principle_matches_by_principle_name").on(t.principleName),
    byFileId: index("principle_matches_by_file_id").on(t.fileId),
    rootNodeFk: foreignKey({
      columns: [t.rootMatchNodeId],
      foreignColumns: [nodes.id],
    }).onDelete("cascade"),
  }),
);

export const principleMatchCaptures = sqliteTable(
  "principle_match_captures",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    matchId: integer("match_id").notNull().references(() => principleMatches.id, { onDelete: "cascade" }),
    captureName: text("capture_name").notNull(),
    capturedNodeId: text("captured_node_id").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    byMatchId: index("principle_match_captures_by_match_id").on(t.matchId),
    matchFk: foreignKey({
      columns: [t.matchId],
      foreignColumns: [principleMatches.id],
    }).onDelete("cascade"),
  }),
);
