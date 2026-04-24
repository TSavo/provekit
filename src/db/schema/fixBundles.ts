import { sqliteTable, integer, text, index, real } from "drizzle-orm/sqlite-core";

export const fixBundles = sqliteTable(
  "fix_bundles",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    bundleType: text("bundle_type").notNull(),
    createdAt: integer("created_at").notNull(),
    signalRawtext: text("signal_rawtext").notNull(),
    signalSource: text("signal_source").notNull(),
    signalSummary: text("signal_summary").notNull(),
    primaryLayer: text("primary_layer").notNull(),
    locusFile: text("locus_file").notNull(),
    locusLine: integer("locus_line").notNull(),
    locusPrimaryNode: text("locus_primary_node"),
    appliedAt: integer("applied_at"),
    commitSha: text("commit_sha"),
    confidence: real("confidence").notNull(),
  },
  (t) => ({
    byBundleType: index("fix_bundles_by_bundle_type").on(t.bundleType),
    byPrimaryLayer: index("fix_bundles_by_primary_layer").on(t.primaryLayer),
  }),
);

export const fixBundleArtifacts = sqliteTable(
  "fix_bundle_artifacts",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    bundleId: integer("bundle_id")
      .notNull()
      .references(() => fixBundles.id, { onDelete: "cascade" }),
    kind: text("kind").notNull(),
    payloadJson: text("payload_json").notNull(),
    passedOracles: text("passed_oracles").notNull(),
    verifiedAt: integer("verified_at").notNull(),
  },
  (t) => ({
    byBundleId: index("fix_bundle_artifacts_by_bundle_id").on(t.bundleId),
    byKindAndBundleId: index("fix_bundle_artifacts_by_kind_bundle_id").on(t.kind, t.bundleId),
  }),
);

export const llmCalls = sqliteTable(
  "llm_calls",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    bundleId: integer("bundle_id")
      .notNull()
      .references(() => fixBundles.id, { onDelete: "cascade" }),
    stage: text("stage").notNull(),
    modelTier: text("model_tier").notNull(),
    prompt: text("prompt").notNull(),
    response: text("response").notNull(),
    seed: integer("seed"),
    ms: integer("ms").notNull(),
    calledAt: integer("called_at").notNull(),
  },
  (t) => ({
    byBundleIdAndStage: index("llm_calls_by_bundle_id_stage").on(t.bundleId, t.stage),
  }),
);

export const pendingFixes = sqliteTable(
  "pending_fixes",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    sourceBundleId: integer("source_bundle_id")
      .notNull()
      .references(() => fixBundles.id, { onDelete: "cascade" }),
    siteNodeId: text("site_node_id").notNull(),
    siteFile: text("site_file").notNull(),
    siteLine: integer("site_line").notNull(),
    reason: text("reason").notNull(),
    priority: integer("priority").notNull(),
    createdAt: integer("created_at").notNull(),
  },
  (t) => ({
    bySourceBundleId: index("pending_fixes_by_source_bundle_id").on(t.sourceBundleId),
    byPriority: index("pending_fixes_by_priority").on(t.priority),
  }),
);
