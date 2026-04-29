/**
 * The memento store — write/read interface for the verifications table.
 *
 * Spec: docs/specs/2026-04-29-relational-memento-store.md
 *
 * v1 scope: schema + insert + lookup + cross-validation join. Cache-
 * miss dispatch and swarm-distribution are later phases. This module
 * is the durable foundation; subsequent phases instrument the C-stages
 * to write mementos and the verifier to read them.
 *
 * The verifications table can live in the existing project DB
 * (`.provekit/provekit.db`) alongside the SAST tables, OR in a separate
 * file (`.provekit/verifications.db`) if we want it independently
 * swarmable. v1 uses the project DB for simplicity; the swarm-
 * distribution phase can move it to a separate file with no API change
 * (this module accepts a Db handle, not a path).
 */

import { and, eq, sql } from "drizzle-orm";
import { createHash } from "crypto";
import type { Db } from "../../db/index.js";
import { verifications } from "../../db/schema/verifications.js";
import type { VerificationRow, VerificationInsert } from "../../db/schema/verifications.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Verdict = "holds" | "violated" | "decayed" | "undecidable" | "error";

export interface Memento {
  bindingHash: string;
  propertyHash: string;
  verdict: Verdict;
  witness?: string | null;
  producedBy: string;
  producedAt?: string;
}

export interface MementoLookupKey {
  bindingHash: string;
  propertyHash: string;
}

// ---------------------------------------------------------------------------
// Hash helpers
// ---------------------------------------------------------------------------

/**
 * Canonical hash for an arbitrary structured value. Stable across runs:
 * stringifies with sorted keys, then sha256-prefix-16. Used by callers
 * that produce mementos to compute consistent property/binding hashes.
 *
 * The producer is responsible for choosing the canonical input — e.g.
 * for a property, hash the Intent IR; for a binding, hash the bound
 * source spans + their structural relationships.
 */
export function hashCanonical(value: unknown): string {
  const canonical = stringifySorted(value);
  return createHash("sha256").update(canonical).digest("hex").slice(0, 16);
}

function stringifySorted(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) {
    return "[" + value.map(stringifySorted).join(",") + "]";
  }
  const keys = Object.keys(value as Record<string, unknown>).sort();
  const entries = keys.map(
    (k) => JSON.stringify(k) + ":" + stringifySorted((value as Record<string, unknown>)[k]),
  );
  return "{" + entries.join(",") + "}";
}

// ---------------------------------------------------------------------------
// Insert
// ---------------------------------------------------------------------------

/**
 * Insert a memento into the verifications table. Idempotent on
 * (bindingHash, propertyHash, producedBy) — re-inserting the same key
 * with a different verdict updates the existing row, surfaces the
 * change in the audit metadata.
 *
 * v1 returns the row inserted/updated; downstream phases use this as
 * the cache-fill operation in the dispatch loop.
 */
export function writeMemento(db: Db, memento: Memento): VerificationRow {
  const producedAt = memento.producedAt ?? new Date().toISOString();
  const row: VerificationInsert = {
    bindingHash: memento.bindingHash,
    propertyHash: memento.propertyHash,
    verdict: memento.verdict,
    witness: memento.witness ?? null,
    producedBy: memento.producedBy,
    producedAt,
    producerSignal: null,
  };

  // Upsert: on conflict (binding_hash, property_hash, produced_by),
  // overwrite verdict + witness + producedAt. The producer's most
  // recent claim wins. Cross-validation against OTHER producers'
  // rows still surfaces via the join in `crossValidate()`.
  db.insert(verifications)
    .values(row)
    .onConflictDoUpdate({
      target: [
        verifications.bindingHash,
        verifications.propertyHash,
        verifications.producedBy,
      ],
      set: {
        verdict: row.verdict,
        witness: row.witness,
        producedAt: row.producedAt,
      },
    })
    .run();

  return findExact(db, memento.bindingHash, memento.propertyHash, memento.producedBy)!;
}

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

/**
 * Cache-lookup query. Returns ANY existing memento for the
 * (bindingHash, propertyHash) pair — the first producer that emitted
 * a row wins. For cross-validation use `findAll()` and join.
 *
 * Caller policy: if multiple producers exist, you may prefer one
 * over another (e.g. prefer "z3-symbolic" over "datalog-structural"
 * for symbolic properties). v1 returns whichever the DB returns first;
 * downstream phases can add producer-priority logic here.
 */
export function findMemento(
  db: Db,
  key: MementoLookupKey,
): Memento | null {
  const rows = db
    .select()
    .from(verifications)
    .where(
      and(
        eq(verifications.bindingHash, key.bindingHash),
        eq(verifications.propertyHash, key.propertyHash),
      ),
    )
    .limit(1)
    .all();
  if (rows.length === 0) return null;
  return rowToMemento(rows[0]);
}

/**
 * All mementos for a (bindingHash, propertyHash) pair, across producers.
 * Used by cross-validation: if multiple rows have different verdicts,
 * the producers disagree — surface as a quality signal.
 */
export function findAll(
  db: Db,
  key: MementoLookupKey,
): Memento[] {
  const rows = db
    .select()
    .from(verifications)
    .where(
      and(
        eq(verifications.bindingHash, key.bindingHash),
        eq(verifications.propertyHash, key.propertyHash),
      ),
    )
    .all();
  return rows.map(rowToMemento);
}

function findExact(
  db: Db,
  bindingHash: string,
  propertyHash: string,
  producedBy: string,
): VerificationRow | null {
  const rows = db
    .select()
    .from(verifications)
    .where(
      and(
        eq(verifications.bindingHash, bindingHash),
        eq(verifications.propertyHash, propertyHash),
        eq(verifications.producedBy, producedBy),
      ),
    )
    .limit(1)
    .all();
  return rows.length > 0 ? rows[0] : null;
}

function rowToMemento(row: VerificationRow): Memento {
  return {
    bindingHash: row.bindingHash,
    propertyHash: row.propertyHash,
    verdict: row.verdict,
    witness: row.witness,
    producedBy: row.producedBy,
    producedAt: row.producedAt,
  };
}

// ---------------------------------------------------------------------------
// Cross-validation
// ---------------------------------------------------------------------------

export interface ProducerDisagreement {
  bindingHash: string;
  propertyHash: string;
  rows: Memento[];
  /** Distinct verdicts emitted across producers for this key. */
  distinctVerdicts: Verdict[];
}

/**
 * Find all (bindingHash, propertyHash) keys where multiple producers
 * have emitted DIFFERENT verdicts. Each disagreement is a producer-
 * quality bug surfaceable mechanically. Quality signals on producers
 * derive from how often they appear in disagreements vs. agreements.
 *
 * v1 implementation: scan + group; sufficient for corpora up to ~100k
 * rows. Larger corpora can switch to a SQL aggregate.
 */
export function crossValidate(db: Db): ProducerDisagreement[] {
  const all = db.select().from(verifications).all();
  const byKey = new Map<string, VerificationRow[]>();
  for (const row of all) {
    const key = `${row.bindingHash}\x1f${row.propertyHash}`;
    const list = byKey.get(key);
    if (list) list.push(row);
    else byKey.set(key, [row]);
  }

  const disagreements: ProducerDisagreement[] = [];
  for (const [, rows] of byKey) {
    if (rows.length < 2) continue;
    const verdicts = new Set<Verdict>(rows.map((r) => r.verdict as Verdict));
    if (verdicts.size < 2) continue;
    disagreements.push({
      bindingHash: rows[0].bindingHash,
      propertyHash: rows[0].propertyHash,
      rows: rows.map(rowToMemento),
      distinctVerdicts: [...verdicts],
    });
  }
  return disagreements;
}

// ---------------------------------------------------------------------------
// Statistics (for verify-cli reporting)
// ---------------------------------------------------------------------------

export interface MementoStoreStats {
  totalRows: number;
  uniqueKeys: number;
  byVerdict: Record<Verdict, number>;
  byProducer: Record<string, number>;
}

export function stats(db: Db): MementoStoreStats {
  const all = db.select().from(verifications).all();
  const keys = new Set<string>();
  const byVerdict = {
    holds: 0,
    violated: 0,
    decayed: 0,
    undecidable: 0,
    error: 0,
  } as Record<Verdict, number>;
  const byProducer: Record<string, number> = {};
  for (const row of all) {
    keys.add(`${row.bindingHash}\x1f${row.propertyHash}`);
    byVerdict[row.verdict as Verdict]++;
    byProducer[row.producedBy] = (byProducer[row.producedBy] ?? 0) + 1;
  }
  return {
    totalRows: all.length,
    uniqueKeys: keys.size,
    byVerdict,
    byProducer,
  };
}
