/**
 * v1 mechanical expressibility tagger for the BugsJS harvest corpus.
 *
 * Two layers, no LLM:
 *   Layer 1 — library coverage: run the current principle library against the
 *   buggy files and check whether any principle fires at locus.
 *   Layer 2 — substrate expressibility: introspect the SAST for the changed
 *   line ranges. Aggregate (capability, column, kind) signatures present on
 *   dirty nodes and detect cross-locus data_flow edges. Bucket the candidate.
 *
 * Per the spec (docs/plans/2026-04-26-principle-tightening.md and the
 * tagger memo): emits an audit line for every tag so the human-review
 * gate (manual-sample-30, ≥90% precision) can diagnose mechanism gaps.
 *
 * Buckets:
 *   "expressible-now-recognized"        — Layer 1 hit
 *   "expressible-now-pending-principle" — substrate coverage ≥ threshold,
 *                                          intra-locus only, no principle yet
 *   "needs-capability-extension"        — dirty nodes have no capability
 *                                          rows; substrate cannot describe shape
 *   "needs-new-relation"                — cross-locus data_flow edges suggest
 *                                          a chain/composition relation absent
 *   "unknown"                           — could not classify mechanically
 *
 * "unknown" is a separate denominator class. If unknown_fraction > 30% on the
 * audit sample, the tagger is the bottleneck and v2 LLM-assist follows.
 */

import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync } from "fs";
import { dirname, join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { fileURLToPath } from "url";
import { openDb } from "../../db/index.js";
import { buildSASTForFile } from "../../sast/builder.js";
import { listCapabilities } from "../../sast/capabilityRegistry.js";
import { recognizeCandidate, parseDiffDirtyLines } from "./recognize.js";
import type { HarvestCandidate } from "./extractBugs.js";

export const TAGGER_VERSION = "v1-mechanical";

export type ExpressibilityBucket =
  | "expressible-now-recognized"
  | "expressible-now-pending-principle"
  | "needs-capability-extension"
  | "needs-new-relation"
  | "unknown";

export interface ExpressibilityTag {
  tag: ExpressibilityBucket;
  layer1Recognized: boolean;
  layer1MatchedPrinciples: string[];
  /** "<capability>.<column>" tuples present on dirty-line nodes. */
  signatureColumns: string[];
  /** "<capability>.<column>:<value>" for kindEnum columns. */
  signatureKinds: string[];
  /** Multi-node relations the bug shape appears to require. */
  signatureRelations: string[];
  missingColumns: string[];
  missingRelations: string[];
  auditLine: string;
  taggerVersion: string;
  taggedAt: string;
}

export interface TagExpressibilityOptions {
  candidate: HarvestCandidate;
  principlesDir?: string;
  scratchParent?: string;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Threshold: number of distinct capabilities with rows on dirty-line nodes
// required to call a candidate "expressible-now-pending-principle". Below
// that threshold we don't have enough signal to combine into a principle;
// the candidate falls to unknown (or needs-new-relation if cross-locus
// signal dominates).
const PENDING_PRINCIPLE_CAPABILITY_THRESHOLD = 2;

// needs-new-relation requires BOTH a high absolute count of cross-locus
// data_flow_transitive edges AND a dominant ratio against intra-locus edges.
// First pass used threshold=1, which fired on nearly every non-trivial fix
// because variables defined outside the diff naturally flow into changes.
// The honest signal is "barely any substrate coverage and the bug is
// dominantly cross-scope" — strong enough to require new relation
// machinery, not just data flow at the edges.
const CROSS_LOCUS_EDGE_ABS_THRESHOLD = 10;
const CROSS_LOCUS_EDGE_RATIO_THRESHOLD = 0.8;

/**
 * Tag a single candidate. Builds a scratch SAST DB, runs both layers, returns
 * the bucket + audit-line. Side-effect-free outside the scratch dir.
 */
export function tagExpressibility(opts: TagExpressibilityOptions): ExpressibilityTag {
  const taggedAt = new Date().toISOString();
  const principlesDir = opts.principlesDir ?? defaultPrinciplesDir();

  // Layer 1: run recognize. If any principle fires at locus, we're done.
  const rec = recognizeCandidate(opts.candidate, { principlesDir, scratchParent: opts.scratchParent });
  if (rec.recognized) {
    const matched = Array.from(new Set(rec.matches.map((m) => m.principleName)));
    return {
      tag: "expressible-now-recognized",
      layer1Recognized: true,
      layer1MatchedPrinciples: matched,
      signatureColumns: [],
      signatureKinds: [],
      signatureRelations: [],
      missingColumns: [],
      missingRelations: [],
      auditLine: `expressible-now-recognized: principles=[${matched.join(",")}] hit at locus`,
      taggerVersion: TAGGER_VERSION,
      taggedAt,
    };
  }

  // Layer 2: rebuild SAST and inspect dirty-line nodes for capability coverage
  // and cross-locus data_flow edges.
  const scratchParent = opts.scratchParent ?? tmpdir();
  const scratchDir = mkdtempSync(join(scratchParent, "provekit-harvest-tag-"));

  let db: ReturnType<typeof openDb> | null = null;
  try {
    const indexedRelPaths: string[] = [];
    for (const [relPath, content] of Object.entries(opts.candidate.buggyFiles)) {
      if (isTestPath(relPath)) continue;
      const abs = join(scratchDir, "src", relPath);
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, content, "utf-8");
      indexedRelPaths.push(relPath);
    }

    if (indexedRelPaths.length === 0) {
      return unknownTag(taggedAt, "no production files to index (test-only candidate)");
    }

    const dbPath = join(scratchDir, "scratch.db");
    db = openDb(dbPath);
    migrate(db, { migrationsFolder: resolveMigrationsDir() });

    const builtRelPaths: string[] = [];
    for (const relPath of indexedRelPaths) {
      const abs = join(scratchDir, "src", relPath);
      try {
        buildSASTForFile(db, abs);
        builtRelPaths.push(relPath);
      } catch {
        // per-file parser failures are tolerated; remaining files still tag
      }
    }

    if (builtRelPaths.length === 0) {
      return unknownTag(taggedAt, "no files indexed (parser failed on every changed file)");
    }

    const dirtyLinesByPath = parseDiffDirtyLines(opts.candidate.diff);
    const dirtyNodeIds = collectDirtyNodeIds(db, scratchDir, builtRelPaths, dirtyLinesByPath);

    if (dirtyNodeIds.size === 0) {
      return unknownTag(taggedAt, "no SAST nodes intersect the diff dirty-line ranges");
    }

    const sig = extractSignature(db, dirtyNodeIds);

    return classify(sig, taggedAt);
  } finally {
    try {
      if (db) db.$client.close();
    } catch {
      /* ignore */
    }
    try {
      rmSync(scratchDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

interface Signature {
  /** "<capability>.<column>" tuples observed. */
  columns: string[];
  /** "<capability>.<column>:<value>" for kindEnum columns. */
  kinds: string[];
  /** Distinct capabilities with at least one row on a dirty node. */
  capabilities: Set<string>;
  /** Number of cross-locus data_flow_transitive edges. */
  crossLocusEdges: number;
  /** Number of intra-locus data_flow_transitive edges. */
  intraLocusEdges: number;
  /** Number of dirty nodes. */
  dirtyNodeCount: number;
  /** Distinct ts-morph kinds observed on dirty nodes. */
  dirtyKinds: string[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function classify(sig: Signature, taggedAt: string): ExpressibilityTag {
  const base = {
    layer1Recognized: false,
    layer1MatchedPrinciples: [],
    signatureColumns: sig.columns,
    signatureKinds: sig.kinds,
    taggerVersion: TAGGER_VERSION,
    taggedAt,
  };

  const totalEdges = sig.crossLocusEdges + sig.intraLocusEdges;
  const crossRatio = totalEdges > 0 ? sig.crossLocusEdges / totalEdges : 0;

  // Default for substrate-covered candidates: pending-principle. The cross-locus
  // signal alone is not enough — variables defined outside the diff flowing in
  // is normal in any non-trivial fix.
  if (sig.capabilities.size >= PENDING_PRINCIPLE_CAPABILITY_THRESHOLD) {
    return {
      ...base,
      tag: "expressible-now-pending-principle",
      signatureRelations: [],
      missingColumns: [],
      missingRelations: [],
      auditLine: `expressible-now-pending-principle: ${sig.capabilities.size} capabilities cover dirty nodes [${[...sig.capabilities].join(",")}]; cross/intra=${sig.crossLocusEdges}/${sig.intraLocusEdges} (${(crossRatio * 100).toFixed(0)}%); no principle fires yet`,
    };
  }

  // 0 capabilities on dirty nodes but dirty nodes exist: substrate doesn't
  // describe these AST shapes. Likely a kind/operator the extractor doesn't
  // cover yet (e.g., array methods like push/splice with no array_op capability).
  if (sig.capabilities.size === 0 && sig.dirtyNodeCount > 0) {
    return {
      ...base,
      tag: "needs-capability-extension",
      signatureRelations: [],
      missingColumns: [`<no capability rows on ${sig.dirtyNodeCount} dirty nodes of kinds [${sig.dirtyKinds.join(",")}]>`],
      missingRelations: [],
      auditLine: `needs-capability-extension: 0 capabilities have rows on ${sig.dirtyNodeCount} dirty nodes; kinds=[${sig.dirtyKinds.join(",")}]`,
    };
  }

  // Barely-covered + dominantly cross-locus: the bug shape mostly reaches
  // outside the diff, suggesting a chain or alias relation is the real
  // missing piece. This is the rare case; most "cross-locus" candidates fall
  // to pending-principle.
  if (
    sig.capabilities.size === 1 &&
    sig.crossLocusEdges >= CROSS_LOCUS_EDGE_ABS_THRESHOLD &&
    crossRatio >= CROSS_LOCUS_EDGE_RATIO_THRESHOLD
  ) {
    return {
      ...base,
      tag: "needs-new-relation",
      signatureRelations: ["data_flow_chain"],
      missingColumns: [],
      missingRelations: ["data_flow_chain (transitive across locus boundary)"],
      auditLine: `needs-new-relation: 1 capability + ${sig.crossLocusEdges}/${totalEdges} cross-locus edges (${(crossRatio * 100).toFixed(0)}%); capabilities=[${[...sig.capabilities].join(",")}]`,
    };
  }

  return {
    ...base,
    tag: "unknown",
    signatureRelations: [],
    missingColumns: [],
    missingRelations: [],
    auditLine: `unknown: ${sig.capabilities.size} capability(ies) on ${sig.dirtyNodeCount} dirty nodes (below threshold ${PENDING_PRINCIPLE_CAPABILITY_THRESHOLD}); kinds=[${sig.dirtyKinds.join(",")}]`,
  };
}

function unknownTag(taggedAt: string, reason: string): ExpressibilityTag {
  return {
    tag: "unknown",
    layer1Recognized: false,
    layer1MatchedPrinciples: [],
    signatureColumns: [],
    signatureKinds: [],
    signatureRelations: [],
    missingColumns: [],
    missingRelations: [],
    auditLine: `unknown: ${reason}`,
    taggerVersion: TAGGER_VERSION,
    taggedAt,
  };
}

function collectDirtyNodeIds(
  db: ReturnType<typeof openDb>,
  scratchDir: string,
  builtRelPaths: string[],
  dirtyLinesByPath: Map<string, Array<[number, number]>>,
): Set<string> {
  const out = new Set<string>();
  const fileIdByPath = new Map<string, number>();
  const fileRows = db.$client
    .prepare(`SELECT id, path FROM files`)
    .all() as { id: number; path: string }[];
  for (const r of fileRows) {
    const rel = relativeToScratch(r.path, scratchDir);
    if (rel) fileIdByPath.set(rel, r.id);
  }

  for (const relPath of builtRelPaths) {
    const ranges = dirtyLinesByPath.get(relPath);
    if (!ranges || ranges.length === 0) continue;
    const fileId = fileIdByPath.get(relPath);
    if (fileId === undefined) continue;

    for (const [start, end] of ranges) {
      const rows = db.$client
        .prepare(`SELECT id FROM nodes WHERE file_id = ? AND source_line >= ? AND source_line <= ?`)
        .all(fileId, start, end) as { id: string }[];
      for (const r of rows) out.add(r.id);
    }
  }
  return out;
}

function extractSignature(
  db: ReturnType<typeof openDb>,
  dirtyNodeIds: Set<string>,
): Signature {
  const sig: Signature = {
    columns: [],
    kinds: [],
    capabilities: new Set(),
    crossLocusEdges: 0,
    intraLocusEdges: 0,
    dirtyNodeCount: dirtyNodeIds.size,
    dirtyKinds: [],
  };

  if (dirtyNodeIds.size === 0) return sig;

  const dirtyArr = [...dirtyNodeIds];
  const placeholders = dirtyArr.map(() => "?").join(",");

  // Distinct ts-morph kinds on dirty nodes.
  const kindRows = db.$client
    .prepare(`SELECT DISTINCT kind FROM nodes WHERE id IN (${placeholders})`)
    .all(...dirtyArr) as { kind: string }[];
  sig.dirtyKinds = kindRows.map((r) => r.kind).sort();

  // Per-capability introspection. For each registered capability, count rows
  // whose node_id (or first node-ref column) lands in the dirty set.
  for (const cap of listCapabilities()) {
    const tableName = (cap.table as unknown as { [Symbol.toStringTag]?: string })?.[Symbol.toStringTag] ?? null;
    const sqlTable = drizzleTableName(cap.table);
    if (!sqlTable) continue;

    const nodeRefCol = pickPrimaryNodeRefColumn(cap);
    if (!nodeRefCol) continue;

    const rows = db.$client
      .prepare(`SELECT * FROM ${sqlTable} WHERE ${nodeRefCol.sqlName} IN (${placeholders})`)
      .all(...dirtyArr) as Record<string, unknown>[];

    if (rows.length === 0) continue;
    sig.capabilities.add(cap.dslName);

    // Per-column observation: which (cap, col) had non-null values, and which
    // kindEnum values appeared.
    for (const [colDslName, colDesc] of Object.entries(cap.columns)) {
      const sqlCol = drizzleColumnName(colDesc.drizzleColumn);
      if (!sqlCol) continue;
      const observed = new Set<unknown>();
      for (const r of rows) {
        const v = r[sqlCol];
        if (v !== null && v !== undefined) observed.add(v);
      }
      if (observed.size === 0) continue;
      sig.columns.push(`${cap.dslName}.${colDslName}`);
      if (colDesc.kindEnum && colDesc.kindEnum.length > 0) {
        for (const v of observed) {
          sig.kinds.push(`${cap.dslName}.${colDslName}:${String(v)}`);
        }
      }
    }
    // tableName lint suppress
    void tableName;
  }
  sig.columns = Array.from(new Set(sig.columns)).sort();
  sig.kinds = Array.from(new Set(sig.kinds)).sort();

  // Cross-locus data_flow_transitive analysis: how many edges have one end
  // in dirty and the other end outside? Strong signal that the bug shape
  // requires a chain relation across the locus boundary.
  const dfRows = db.$client
    .prepare(
      `SELECT to_node, from_node FROM data_flow_transitive
       WHERE to_node IN (${placeholders}) OR from_node IN (${placeholders})`,
    )
    .all(...dirtyArr, ...dirtyArr) as { to_node: string; from_node: string }[];
  for (const r of dfRows) {
    const toIn = dirtyNodeIds.has(r.to_node);
    const fromIn = dirtyNodeIds.has(r.from_node);
    if (toIn && fromIn) sig.intraLocusEdges++;
    else sig.crossLocusEdges++;
  }

  return sig;
}

function pickPrimaryNodeRefColumn(cap: ReturnType<typeof listCapabilities>[number]):
  | { dslName: string; sqlName: string }
  | null {
  // Prefer a column literally named node_id (the canonical "row applies to
  // this node" convention used across the registered capabilities).
  for (const [dslName, desc] of Object.entries(cap.columns)) {
    if (!desc.isNodeRef) continue;
    if (dslName === "node_id") {
      const sqlName = drizzleColumnName(desc.drizzleColumn);
      if (sqlName) return { dslName, sqlName };
    }
  }
  // Fallback to the first node-ref column in declaration order.
  for (const [dslName, desc] of Object.entries(cap.columns)) {
    if (!desc.isNodeRef) continue;
    const sqlName = drizzleColumnName(desc.drizzleColumn);
    if (sqlName) return { dslName, sqlName };
  }
  return null;
}

function drizzleTableName(table: unknown): string | null {
  // Drizzle stores the SQL table name on a well-known symbol; access it
  // defensively because the symbol identity isn't part of the public API.
  if (!table || typeof table !== "object") return null;
  const t = table as Record<string | symbol, unknown>;
  for (const sym of Object.getOwnPropertySymbols(t)) {
    if (String(sym).includes("Name")) {
      const v = t[sym];
      if (typeof v === "string") return v;
    }
  }
  return null;
}

function drizzleColumnName(col: unknown): string | null {
  if (!col || typeof col !== "object") return null;
  const c = col as Record<string, unknown>;
  if (typeof c["name"] === "string") return c["name"] as string;
  return null;
}

function isTestPath(p: string): boolean {
  const segments = p.split("/");
  for (const seg of segments) {
    if (seg === "test" || seg === "tests" || seg === "__tests__") return true;
  }
  return /\.(test|spec)\.[^/]+$/.test(p);
}

function relativeToScratch(absPath: string, scratchDir: string): string | null {
  const prefix = join(scratchDir, "src") + "/";
  if (!absPath.startsWith(prefix)) return null;
  return absPath.slice(prefix.length);
}

function defaultPrinciplesDir(): string {
  return join(__dirname, "..", "..", "..", ".provekit", "principles");
}

function resolveMigrationsDir(): string {
  return join(__dirname, "..", "..", "..", "drizzle");
}
