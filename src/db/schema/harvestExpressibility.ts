import {
  sqliteTable,
  integer,
  text,
  index,
  primaryKey,
} from "drizzle-orm/sqlite-core";

/**
 * Per-candidate expressibility tag for the BugsJS harvest corpus. Populated
 * by the v1 mechanical tagger (src/fix/harvest/expressibility.ts). Each row
 * captures whether a candidate is reachable by the current substrate, plus
 * the audit-line and signature evidence that explains the bucket choice.
 *
 * The tag migrates as the substrate gains capabilities: a candidate
 * previously tagged needs-new-relation can become expressible-now-pending-principle
 * once the relation is added. Re-running the tagger against the same corpus
 * is the canonical way to measure substrate-extension's effect on coverage.
 */
export const harvestExpressibility = sqliteTable(
  "harvest_expressibility",
  {
    project: text("project").notNull(),
    bugId: text("bug_id").notNull(),
    /**
     * One of:
     *   - "expressible-now-recognized"     : Layer 1 hit, library matches at locus
     *   - "expressible-now-pending-principle" : Layer 2: substrate covers signature, no principle yet
     *   - "needs-capability-extension"      : almost-existing column or kind missing
     *   - "needs-new-relation"              : multi-node relation absent (chain, alias, composition)
     *   - "unknown"                         : tagger could not classify mechanically
     */
    tag: text("tag").notNull(),
    layer1Recognized: integer("layer1_recognized", { mode: "boolean" }).notNull(),
    /** Names of principles that fired at locus, JSON array. Empty when not recognized. */
    layer1MatchedPrinciples: text("layer1_matched_principles").notNull().default("[]"),
    /** "<capability>.<column>" tuples present on changed-line nodes, JSON array. */
    signatureColumns: text("signature_columns").notNull().default("[]"),
    /** Kind values observed (e.g., "arithmetic.op:/"). JSON array. */
    signatureKinds: text("signature_kinds").notNull().default("[]"),
    /** Multi-node relations the bug shape appears to require, JSON array. */
    signatureRelations: text("signature_relations").notNull().default("[]"),
    /** Capability columns that would be needed but aren't registered. JSON array. */
    missingColumns: text("missing_columns").notNull().default("[]"),
    /** Relations the bug shape needs that the substrate doesn't have. JSON array. */
    missingRelations: text("missing_relations").notNull().default("[]"),
    /** Human-readable single line: "<bucket> because <signature> vs <coverage>". */
    auditLine: text("audit_line").notNull(),
    /** Bug-shape sig version (e.g., "v1-mechanical"). Bumps when the tagger changes. */
    taggerVersion: text("tagger_version").notNull(),
    taggedAt: text("tagged_at").notNull(),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.project, t.bugId] }),
    byTag: index("harvest_expressibility_by_tag").on(t.tag),
    byTaggerVersion: index("harvest_expressibility_by_tagger_version").on(t.taggerVersion),
  }),
);
