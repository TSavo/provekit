import { sqliteTable, integer, text, index } from "drizzle-orm/sqlite-core";

/**
 * Per-node diff classification harvested from a (pre, post) file pair.
 * Populated at harvest time by walking each candidate's buggy/fixed file
 * pair through `computeFileDiff` (src/sast/diff.ts).
 *
 * One row per AST node on either side. For "unchanged" and "modified",
 * both pre and post sides are populated. For "added", only the post side
 * is populated; for "deleted", only the pre side.
 *
 * Day 3 substrate use: a DSL relation will look up by (project, candidate_id,
 * file_path, post_start, post_kind) — the post-side coordinates locate a
 * node in the post-fix SAST and the row tells us what diff bucket it
 * landed in. Indexed accordingly.
 *
 * 2026-04-27: hard-bug 1 Day 2 deliverable.
 */
export const prePostDiff = sqliteTable(
  "pre_post_diff",
  {
    /** Diff context: "harvest:<project>:<bugId>" or "lint:<refA>:<refB>". */
    context: text("context").notNull(),
    /** File the diff was computed for (relative to the project root). */
    filePath: text("file_path").notNull(),
    /** unchanged | modified | added | deleted */
    changeKind: text("change_kind").notNull(),

    // Pre-side coordinates (null for "added").
    preFingerprint: text("pre_fingerprint"),
    preParentFingerprint: text("pre_parent_fingerprint"),
    preOrdinal: integer("pre_ordinal"),
    preKind: text("pre_kind"),
    preLine: integer("pre_line"),
    preCol: integer("pre_col"),
    preStart: integer("pre_start"),
    preEnd: integer("pre_end"),
    preTextPreview: text("pre_text_preview"),

    // Post-side coordinates (null for "deleted").
    postFingerprint: text("post_fingerprint"),
    postParentFingerprint: text("post_parent_fingerprint"),
    postOrdinal: integer("post_ordinal"),
    postKind: text("post_kind"),
    postLine: integer("post_line"),
    postCol: integer("post_col"),
    postStart: integer("post_start"),
    postEnd: integer("post_end"),
    postTextPreview: text("post_text_preview"),
  },
  (t) => ({
    byContext: index("pre_post_diff_by_context").on(t.context),
    byContextChange: index("pre_post_diff_by_context_change").on(t.context, t.changeKind),
    // Locate-by-post-position: the most common DSL-relation lookup pattern.
    byPostLocation: index("pre_post_diff_by_post_location").on(
      t.context,
      t.filePath,
      t.postStart,
      t.postKind,
    ),
  }),
);
