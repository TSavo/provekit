import { sqliteTable, integer, text, index } from "drizzle-orm/sqlite-core";
import { fixBundles } from "./fixBundles.js";

export const principlesLibrary = sqliteTable(
  "principles_library",
  {
    name: text("name").primaryKey(),
    dslPath: text("dsl_path").notNull(),
    jsonPath: text("json_path").notNull(),
    confidenceTier: text("confidence_tier").notNull(), // "advisory" | "warning" | "blocking"
    addedBundleId: integer("added_bundle_id").references(() => fixBundles.id, {
      onDelete: "set null",
    }),
    addedAt: integer("added_at").notNull(),
    falseNegativeCount: integer("false_negative_count").notNull().default(0),
    successfulApplicationCount: integer("successful_application_count")
      .notNull()
      .default(0),
  },
  (t) => ({
    byConfidenceTier: index("principles_library_by_confidence_tier").on(
      t.confidenceTier,
    ),
    byAddedBundleId: index("principles_library_by_added_bundle_id").on(
      t.addedBundleId,
    ),
  }),
);
