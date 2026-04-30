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
import type { ClaimEnvelope, EvidenceVariant } from "../../claimEnvelope/index.js";
import { computeEnvelopeCid } from "../../claimEnvelope/index.js";
import { VARIANT_SCHEMA_CIDS } from "../../claimEnvelope/variants/index.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Verdict = "holds" | "violated" | "decayed" | "undecidable" | "error";

/**
 * Caller-supplied hint that selects an evidence variant for the envelope.
 * The wrapper (bindingHash, propertyHash, verdict, producedBy, inputCids)
 * is filled by writeMemento; the producer only specifies the variant.
 *
 * If no hint is supplied, writeMemento wraps the raw `witness` string in
 * a `legacy-witness` variant.
 */
export type EvidenceHint =
  | { kind: "z3-model"; body: Extract<EvidenceVariant, { kind: "z3-model" }>["body"] }
  | { kind: "z3-unsat"; body: Extract<EvidenceVariant, { kind: "z3-unsat" }>["body"] }
  | { kind: "pattern-match"; body: Extract<EvidenceVariant, { kind: "pattern-match" }>["body"] }
  | { kind: "type-check-pass"; body: Extract<EvidenceVariant, { kind: "type-check-pass" }>["body"] }
  | { kind: "lint-pass"; body: Extract<EvidenceVariant, { kind: "lint-pass" }>["body"] }
  | { kind: "test-pass"; body: Extract<EvidenceVariant, { kind: "test-pass" }>["body"] }
  | { kind: "test-fail"; body: Extract<EvidenceVariant, { kind: "test-fail" }>["body"] }
  | { kind: "llm-proposal"; body: Extract<EvidenceVariant, { kind: "llm-proposal" }>["body"] }
  | { kind: "mutation-witness"; body: Extract<EvidenceVariant, { kind: "mutation-witness" }>["body"] }
  | { kind: "workflow-run"; body: Extract<EvidenceVariant, { kind: "workflow-run" }>["body"] };

export interface Memento {
  bindingHash: string;
  propertyHash: string;
  verdict: Verdict;
  witness?: string | null;
  producedBy: string;
  producedAt?: string;
  /**
   * Canonical CID for this memento (sha256-prefix-32 of the
   * canonicalized payload). Computed at write time; stable across
   * runs that produce the same content.
   */
  cid?: string;
  /**
   * CIDs of upstream mementos that fed this memento's production.
   * Empty for terminal mementos. Walking these CIDs reconstructs
   * the provenance DAG.
   */
  inputCids?: string[];
  /**
   * Caller-supplied evidence-variant hint. When present, writeMemento
   * builds a typed envelope variant (z3-model, llm-proposal, etc.)
   * instead of the default `legacy-witness` wrapper around the raw
   * witness string.
   *
   * On read, mementos with a typed variant expose the parsed evidence
   * via `evidence`; the raw `witness` field will be null because the
   * payload now lives inside `evidence.body`.
   */
  evidenceHint?: EvidenceHint;
  /**
   * Parsed evidence variant (set by rowToMemento when the row's
   * stored payload is a typed claim envelope). Producers do not set
   * this on writes — use `evidenceHint` instead.
   */
  evidence?: EvidenceVariant;
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
 * Compute the canonical CID for a memento. sha256-prefix-32 of the
 * canonicalized payload — binding_hash, property_hash, verdict,
 * witness, produced_by, and the sorted input_cids list. Stable across
 * runs that produce the same content; collision-resistant at any
 * plausible corpus scale.
 *
 * 32 hex chars (16 bytes / 128 bits) is enough for the non-adversarial
 * case. Lengthen to 40 or 64 for adversarial-resistance deployments.
 */
export function computeCid(memento: Memento): string {
  const canonical = stringifySorted({
    binding_hash: memento.bindingHash,
    property_hash: memento.propertyHash,
    verdict: memento.verdict,
    witness: memento.witness ?? null,
    produced_by: memento.producedBy,
    input_cids: [...(memento.inputCids ?? [])].sort(),
  });
  return createHash("sha256").update(canonical).digest("hex").slice(0, 32);
}

/**
 * Build the evidence variant for an envelope. When the producer supplies
 * an `evidenceHint`, that variant is used; otherwise the raw `witness`
 * string is wrapped in a `legacy-witness` variant.
 */
function buildEvidence(memento: Memento): EvidenceVariant {
  if (memento.evidenceHint) {
    const hint = memento.evidenceHint;
    return {
      kind: hint.kind,
      schema: VARIANT_SCHEMA_CIDS[hint.kind],
      body: hint.body,
    } as EvidenceVariant;
  }
  return {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"],
    body: {
      rawWitness: memento.witness ?? "",
      legacyProducerId: memento.producedBy,
    },
  };
}

/**
 * Insert a memento into the verifications table. Idempotent on
 * (bindingHash, propertyHash, producedBy) — re-inserting the same key
 * with a different verdict updates the existing row, surfaces the
 * change in the audit metadata.
 *
 * The row's `witness` column stores the canonicalized claim envelope
 * (per docs/specs/2026-04-29-universal-claim-envelope.md). The CID
 * column is the envelope CID — sha256-prefix-32 of the canonicalized
 * envelope with `cid` and `producerSignature` elided. Producers that
 * want a typed evidence variant set `evidenceHint` on the memento;
 * legacy callers continue to pass `witness` as a string and the
 * envelope wraps it in a `legacy-witness` variant.
 */
export function writeMemento(db: Db, memento: Memento): VerificationRow {
  const producedAt = memento.producedAt ?? new Date().toISOString();
  const sortedInputCids = [...(memento.inputCids ?? [])].sort();
  const evidence = buildEvidence(memento);
  const envelopeBase: Omit<ClaimEnvelope, "cid"> = {
    schemaVersion: "1",
    bindingHash: memento.bindingHash,
    propertyHash: memento.propertyHash,
    verdict: memento.verdict,
    producedBy: memento.producedBy,
    producedAt,
    inputCids: sortedInputCids,
    evidence,
  };
  const cid = memento.cid ?? computeEnvelopeCid(envelopeBase);
  const envelope: ClaimEnvelope = { ...envelopeBase, cid };
  const witnessJson = JSON.stringify(envelope);
  const inputCidsJson =
    sortedInputCids.length > 0 ? JSON.stringify(sortedInputCids) : null;
  const row: VerificationInsert = {
    bindingHash: memento.bindingHash,
    propertyHash: memento.propertyHash,
    verdict: memento.verdict,
    witness: witnessJson,
    producedBy: memento.producedBy,
    producedAt,
    producerSignal: null,
    cid,
    inputCids: inputCidsJson,
  };

  // Upsert: on conflict (binding_hash, property_hash, produced_by),
  // overwrite verdict + witness + producedAt + cid + input_cids.
  // Producer's most recent claim wins on the row's identity tuple;
  // historical rows for OTHER producers stay for cross-validation.
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
        cid: row.cid,
        inputCids: row.inputCids,
      },
    })
    .run();

  return findExact(db, memento.bindingHash, memento.propertyHash, memento.producedBy)!;
}

/**
 * Fetch a memento by its CID. Iterates the table — for v1 corpus
 * sizes this is fine; later we'll lean on the cid index for direct
 * lookup. Returns null if no row matches.
 */
export function findByCid(db: Db, cid: string): Memento | null {
  const rows = db
    .select()
    .from(verifications)
    .where(eq(verifications.cid, cid))
    .limit(1)
    .all();
  if (rows.length === 0) return null;
  return rowToMemento(rows[0]);
}

/**
 * Walk the DAG of inputs from a starting CID. Returns the starting
 * memento + all transitively-reachable upstream mementos in BFS
 * order, capped at maxDepth. The operational form of "audit trail =
 * DAG walk" — given any memento, reconstruct the chain of mementos
 * that produced it.
 *
 * Cycles are detected via a visited-set (CIDs are unique; a real
 * Merkle DAG can't have cycles, but defensive against corruption).
 */
export function walk(
  db: Db,
  startCid: string,
  opts: { maxDepth?: number } = {},
): Memento[] {
  const maxDepth = opts.maxDepth ?? 100;
  const visited = new Set<string>();
  const out: Memento[] = [];
  const queue: Array<{ cid: string; depth: number }> = [
    { cid: startCid, depth: 0 },
  ];

  while (queue.length > 0) {
    const { cid, depth } = queue.shift()!;
    if (visited.has(cid)) continue;
    visited.add(cid);
    const memento = findByCid(db, cid);
    if (!memento) continue;
    out.push(memento);
    if (depth >= maxDepth) continue;
    for (const inputCid of memento.inputCids ?? []) {
      if (!visited.has(inputCid)) {
        queue.push({ cid: inputCid, depth: depth + 1 });
      }
    }
  }
  return out;
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

/**
 * All mementos sharing a bindingHash, optionally filtered by producedBy.
 *
 * Producer-key management uses this to walk rotation chains: rotation
 * mementos encode oldKeyCid into their bindingHash, and the chain walker
 * needs to find a rotation memento by bindingHash alone (the propertyHash
 * encodes the unknown new key bytes).
 */
export function findMementoByBindingHash(
  db: Db,
  bindingHash: string,
  opts: { producedBy?: string } = {},
): Memento[] {
  const where = opts.producedBy
    ? and(
        eq(verifications.bindingHash, bindingHash),
        eq(verifications.producedBy, opts.producedBy),
      )
    : eq(verifications.bindingHash, bindingHash);
  const rows = db.select().from(verifications).where(where).all();
  return rows.map(rowToMemento);
}

/**
 * All mementos sharing a propertyHash, across producers and bindings.
 *
 * The refute workflow uses this: given a propertyHash, find any
 * memento claiming that property — typically a formulate-via-lifter
 * legacy-witness memento that carries the IrFormula in its rawWitness.
 * The first row's binding identifies the code shape under test.
 */
export function findMementoByPropertyHash(
  db: Db,
  propertyHash: string,
): Memento[] {
  const rows = db
    .select()
    .from(verifications)
    .where(eq(verifications.propertyHash, propertyHash))
    .all();
  return rows.map(rowToMemento);
}

/**
 * All mementos stored locally. Used by leaves/roots enumeration: the
 * proofkit's local mementos are what `provekit leaves` lists, and the
 * set difference between their inputCids and their CIDs is what
 * `provekit roots` lists. Per the substrate-vs-tooling boundary in
 * docs/specs/2026-04-29-correctness-is-a-hash.md §"Naming discipline:
 * leaves AND roots, not walks", both queries are LOCAL by design.
 */
export function listAllMementos(db: Db): Memento[] {
  const rows = db.select().from(verifications).all();
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

/**
 * Try to parse the witness column as a claim envelope. Returns null on
 * any failure — the row is then treated as a legacy pre-envelope row
 * whose witness column held opaque producer JSON.
 */
function tryParseEnvelope(witness: string | null): ClaimEnvelope | null {
  if (witness == null) return null;
  if (witness.length === 0 || witness[0] !== "{") return null;
  try {
    const parsed = JSON.parse(witness) as Partial<ClaimEnvelope>;
    if (
      parsed &&
      typeof parsed === "object" &&
      parsed.schemaVersion === "1" &&
      typeof parsed.bindingHash === "string" &&
      typeof parsed.propertyHash === "string" &&
      typeof parsed.evidence === "object" &&
      parsed.evidence !== null
    ) {
      return parsed as ClaimEnvelope;
    }
  } catch {
    // fall through
  }
  return null;
}

function rowToMemento(row: VerificationRow): Memento {
  const inputCids = row.inputCids ? (JSON.parse(row.inputCids) as string[]) : undefined;
  const envelope = tryParseEnvelope(row.witness);

  // Default: legacy row, witness column holds opaque producer JSON.
  let witness: string | null = row.witness;
  let evidence: EvidenceVariant | undefined;

  if (envelope) {
    evidence = envelope.evidence;
    if (envelope.evidence.kind === "legacy-witness") {
      // Surface the wrapped raw payload for callers that read .witness.
      witness = envelope.evidence.body.rawWitness;
    } else {
      // Typed variant: witness lives in evidence.body, not .witness.
      witness = null;
    }
  }

  return {
    bindingHash: row.bindingHash,
    propertyHash: row.propertyHash,
    verdict: row.verdict,
    witness,
    producedBy: row.producedBy,
    producedAt: row.producedAt,
    cid: row.cid ?? undefined,
    inputCids,
    evidence,
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

// ---------------------------------------------------------------------------
// Hash computation for invariants (used by step-2 instrumentation)
// ---------------------------------------------------------------------------

/**
 * Compute the binding_hash for a StoredInvariant — fingerprints the
 * code shape the invariant binds to. Stable across runs as long as
 * the bound source spans (and their structural relationships) are
 * unchanged. Two invariants whose bindings reference the same code
 * shape collapse to the same binding_hash.
 *
 * v1 implementation: hash the array of bindings normalized to the
 * fields that determine "what code is bound" — local bindings hash
 * their nodeHash + filePath + line range; graph bindings hash root
 * + relation + predicate. The smt_constant aliases are NOT included
 * (they're arbitrary names that don't change the bound code).
 */
export function computeBindingHash(invariant: {
  bindings: ReadonlyArray<unknown>;
}): string {
  const normalized = (invariant.bindings as Array<Record<string, unknown>>).map((b) => {
    if (b.type === "graph") {
      return {
        type: "graph",
        relation: b.relation,
        root: b.root,
        predicate: b.predicate,
        predicateArg: b.predicateArg,
      };
    }
    // Local binding (default for legacy bindings without a type field).
    return {
      type: "local",
      filePath: (b as { node?: { filePath?: string } }).node?.filePath,
      nodeHash: (b as { node?: { nodeHash?: string } }).node?.nodeHash,
      startLine: (b as { node?: { startLine?: number } }).node?.startLine,
      endLine: (b as { node?: { endLine?: number } }).node?.endLine,
    };
  });
  // Sort by JSON-stringified form so the hash is binding-order-independent.
  normalized.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return hashCanonical({ bindings: normalized });
}

/**
 * Compute the property_hash for a StoredInvariant — fingerprints the
 * formal claim being checked, independent of which code it binds to.
 * Two invariants with the same SMT formula on different code share
 * the same property_hash.
 */
export function computePropertyHash(invariant: {
  smt: { kind: string; declarations: string[]; assertion: string };
}): string {
  return hashCanonical({
    kind: invariant.smt.kind,
    declarations: [...invariant.smt.declarations].sort(),
    assertion: invariant.smt.assertion,
  });
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
