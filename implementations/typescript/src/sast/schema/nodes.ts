import {
  sqliteTable,
  integer,
  text,
  index,
  foreignKey,
} from "drizzle-orm/sqlite-core";

export const files = sqliteTable(
  "files",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    path: text("path").notNull().unique(),
    contentHash: text("content_hash").notNull(),
    parsedAt: integer("parsed_at").notNull(),
    rootNodeId: text("root_node_id").notNull().default(""),
  },
  (t) => ({
    byContentHash: index("files_by_content_hash").on(t.contentHash),
  }),
);

export const nodes = sqliteTable(
  "nodes",
  {
    id: text("id").primaryKey(),
    fileId: integer("file_id").notNull(),
    sourceStart: integer("source_start").notNull(),
    sourceEnd: integer("source_end").notNull(),
    sourceLine: integer("source_line").notNull(),
    sourceCol: integer("source_col").notNull(),
    subtreeHash: text("subtree_hash").notNull(),
    kind: text("kind").notNull(),
  },
  (t) => ({
    byFileId: index("nodes_by_file_id").on(t.fileId),
    bySubtreeHash: index("nodes_by_subtree_hash").on(t.subtreeHash),
    fileFk: foreignKey({
      columns: [t.fileId],
      foreignColumns: [files.id],
    }).onDelete("cascade"),
  }),
);
