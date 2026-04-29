/**
 * Verifications table — the relational memento store.
 *
 * Spec: docs/specs/2026-04-29-relational-memento-store.md
 *
 * Each row is a deterministic verdict on a (binding_hash, property_hash)
 * pair, produced by some named producer. The framework's runtime work
 * is hash lookup against this table; engines fire only on cache miss.
 *
 * Schema design:
 * - Composite PK on (binding_hash, property_hash, produced_by) so
 *   multiple producers can each contribute their own row for the same
 *   key. Cross-validation = JOIN on (binding_hash, property_hash) and
 *   look for verdict disagreements.
 * - Index on (binding_hash, property_hash) for the lookup query
 *   (the hot path; the cache hit case).
 * - Witness stored as JSON text — Z3 model, SQL match rows, mutation
 *   trace. Producer-specific shape; the framework treats it as opaque.
 * - producer_signal: pass / fail / quality from bp telemetry.
 */

import { sqliteTable, text, integer, primaryKey, index } from "drizzle-orm/sqlite-core";

export const verifications = sqliteTable(
  "verifications",
  {
    /**
     * sha256 prefix (16 hex chars) of the bound source spans + their
     * structural relationships. Identifies the code shape the verdict
     * applies to. Same code = same hash = same row.
     */
    bindingHash: text("binding_hash").notNull(),
    /**
     * sha256 of the canonicalized Intent IR for the property — kind,
     * bindings spec, property expression. Same property = same hash =
     * same row across producer reruns.
     */
    propertyHash: text("property_hash").notNull(),
    /**
     * Verdict: "holds" | "violated" | "decayed" | "undecidable" | "error".
     * "decayed" surfaces when the binding hash has changed since mint;
     * "undecidable" when the producer can't resolve (e.g. Z3 timeout).
     */
    verdict: text("verdict", {
      enum: ["holds", "violated", "decayed", "undecidable", "error"],
    }).notNull(),
    /**
     * Producer-emitted artifact justifying the verdict. JSON-shaped:
     * Z3 model dump, SQL match rows, mutation-test stdout, etc. The
     * framework treats this as opaque; producers and audit consumers
     * read it.
     */
    witness: text("witness"),
    /**
     * Producer identity: "z3-symbolic@4.13", "datalog-structural@1.0",
     * "llm:claude-opus-4-7", "mutation-test@1.0", etc. Versioned so
     * that engine upgrades are visible in the audit trail.
     */
    producedBy: text("produced_by").notNull(),
    /**
     * ISO-8601 timestamp.
     */
    producedAt: text("produced_at").notNull(),
    /**
     * Optional bp signal for the producer that emitted this row.
     * "pass" | "fail" | "unknown" depending on whether the verdict
     * later got contradicted or confirmed by a cross-validating
     * producer.
     */
    producerSignal: text("producer_signal", {
      enum: ["pass", "fail", "unknown"],
    }),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.bindingHash, t.propertyHash, t.producedBy] }),
    lookup: index("verifications_lookup").on(t.bindingHash, t.propertyHash),
  }),
);

/**
 * Inferred row type from the schema. Use this when constructing
 * mementos in code so the field shape stays in sync with the schema.
 */
export type VerificationRow = typeof verifications.$inferSelect;
export type VerificationInsert = typeof verifications.$inferInsert;
