/**
 * B3: Recognize.
 *
 * The principle library IS the bug-recognition mechanism. Every principle is
 * a compiled SAST query. Recognition is a database query that runs in
 * microseconds, not an LLM judgment.
 *
 * B3 sits between Locate and the C-stages. It runs every principle's DSL
 * against the locus's file, finds the highest-confidence match whose root
 * intersects locus.primaryNode, and (if found) hands the matched
 * LibraryPrinciple plus its bindings to the orchestrator. The orchestrator
 * then routes C1/C3/C5/C6 through their mechanical-mode arms (C1m, C3m, C5m,
 * C6m), bypassing all LLM calls on the recognized path.
 *
 * Algorithm:
 *   1. Read every JSON file in `.provekit/principles/` as a LibraryPrinciple.
 *   2. Skip principles missing `fixTemplate` or `testTemplate` (log warning).
 *   3. Ensure the DSL evaluator has populated `principleMatches` for the
 *      locus's file — same lazy-population pattern that formulateInvariant
 *      Path 1 uses.
 *   4. Query `principleMatches` for rows whose rootMatchNode intersects the
 *      locus's primary node (exact-match preferred, span-containment fallback).
 *   5. Among intersecting rows, prefer principles whose JSON has both
 *      fixTemplate AND testTemplate (mechanical-ready). Fall back to LLM
 *      mode if a match exists but the principle isn't ready.
 *   6. Among multiple ready matches, pick the highest confidence
 *      ("high" > "medium" > "low" > unspecified). Ties broken by first match.
 *
 * Wall-time target: a few milliseconds at library size 20-200 principles
 * across a typical 1000-LOC file. No LLM calls. No Z3.
 */

import { readFileSync, existsSync, readdirSync } from "fs";
import { join, dirname } from "path";
import { eq, and, lte, gte } from "drizzle-orm";
import type { Db } from "../../db/index.js";
import type { BugLocus, LibraryPrinciple } from "../types.js";
import { principleMatches, principleMatchCaptures } from "../../db/schema/principleMatches.js";
import { nodes } from "../../sast/schema/index.js";
import { evaluatePrinciple } from "../../dsl/evaluator.js";
import { createNoopLogger, type FixLoopLogger } from "../logger.js";

// ---------------------------------------------------------------------------
// Result shape
// ---------------------------------------------------------------------------

export type RecognizeResult =
  | { matched: false }
  | {
      matched: true;
      principleId: string;
      bugClassId: string;
      /** Capture name → matched node ID. Drives downstream template binding. */
      bindings: Record<string, string>;
      /** The full in-memory LibraryPrinciple, ready for mechanical-mode use. */
      principle: LibraryPrinciple;
      /** principleMatches.id of the row that fired — for downstream lookup. */
      matchId: number;
      /** Root SAST node ID of the principle match (for debug / audit). */
      rootMatchNodeId: string;
    };

// ---------------------------------------------------------------------------
// Principle directory + loader
// ---------------------------------------------------------------------------

/**
 * Resolve `.provekit/principles/` by walking up from this module.
 *
 * Test override: `PROVEKIT_PRINCIPLES_DIR` env var, when set, takes precedence.
 * This lets tests redirect provenance writes (C6m) to a scratch directory
 * rather than mutating the canonical repo principles.
 */
export function findPrinciplesDir(): string {
  const envOverride = process.env.PROVEKIT_PRINCIPLES_DIR;
  if (envOverride && existsSync(envOverride)) return envOverride;

  let dir = __dirname;
  for (let i = 0; i < 10; i++) {
    const candidate = join(dir, ".provekit", "principles");
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return join(process.cwd(), ".provekit", "principles");
}

/**
 * Library snapshot. Loaded on demand by recognize() each call. v1 is fine
 * with no caching: 20-200 small JSON files, all on local disk, total < 100KB.
 */
export interface PrincipleLibrary {
  /** Map principle id → in-memory principle. */
  byId: Map<string, LibraryPrinciple>;
  /** Source directory the library was loaded from. */
  dir: string;
}

export function loadPrincipleLibrary(dir?: string): PrincipleLibrary {
  const principlesDir = dir ?? findPrinciplesDir();
  const byId = new Map<string, LibraryPrinciple>();

  if (!existsSync(principlesDir)) {
    return { byId, dir: principlesDir };
  }

  let entries: string[];
  try {
    entries = readdirSync(principlesDir).filter((f) => f.endsWith(".json"));
  } catch {
    return { byId, dir: principlesDir };
  }

  for (const entry of entries) {
    const path = join(principlesDir, entry);
    let raw: string;
    try {
      raw = readFileSync(path, "utf-8");
    } catch {
      continue;
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      continue;
    }
    if (!isLibraryPrinciple(parsed)) continue;
    byId.set(parsed.id, parsed);
  }

  return { byId, dir: principlesDir };
}

function isLibraryPrinciple(x: unknown): x is LibraryPrinciple {
  if (!x || typeof x !== "object") return false;
  const obj = x as Record<string, unknown>;
  return typeof obj.id === "string" && typeof obj.bug_class_id === "string";
}

// ---------------------------------------------------------------------------
// Principle-match population (shared with formulateInvariant Path 1)
// ---------------------------------------------------------------------------

/**
 * Lazy-populate principleMatches for the locus file. Idempotent: a non-empty
 * file row count returns immediately. Errors are swallowed (per-DSL skip with
 * a detail log) — the recognized path is opportunistic.
 */
function ensurePrincipleMatchesPopulated(
  db: Db,
  locusFileId: number,
  principlesDir: string,
  logger: FixLoopLogger,
): void {
  const existing = db
    .select({ id: principleMatches.id })
    .from(principleMatches)
    .where(eq(principleMatches.fileId, locusFileId))
    .limit(1)
    .all();
  if (existing.length > 0) return;

  if (!existsSync(principlesDir)) return;

  let dslFiles: string[];
  try {
    dslFiles = readdirSync(principlesDir).filter((f) => f.endsWith(".dsl"));
  } catch {
    return;
  }

  for (const dslFile of dslFiles) {
    const dslPath = join(principlesDir, dslFile);
    let dslSource: string;
    try {
      dslSource = readFileSync(dslPath, "utf-8");
    } catch {
      continue;
    }
    try {
      evaluatePrinciple(db, dslSource);
    } catch (err) {
      logger.detail(
        `[B3] principle ${dslFile} eval skipped: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Confidence ranking
// ---------------------------------------------------------------------------

const CONFIDENCE_RANK: Record<string, number> = {
  high: 3,
  medium: 2,
  low: 1,
};

function confidenceScore(p: LibraryPrinciple): number {
  if (!p.confidence) return 0;
  return CONFIDENCE_RANK[p.confidence] ?? 0;
}

// ---------------------------------------------------------------------------
// Main entry
// ---------------------------------------------------------------------------

export interface RecognizeArgs {
  db: Db;
  locus: BugLocus;
  /** Preloaded library. If absent, recognize() loads from `.provekit/principles/`. */
  library?: PrincipleLibrary;
  /** Logger; defaults to noop. */
  logger?: FixLoopLogger;
}

/**
 * Run recognition against the locus. Pure SAST + DSL evaluation. No LLM.
 * Returns `{ matched: false }` when no library principle matches OR when
 * matches exist but none have both fixTemplate and testTemplate.
 *
 * Test escape hatch: `PROVEKIT_DISABLE_RECOGNIZE=1` short-circuits to
 * `{ matched: false }` without loading the library. Tests that exercise
 * the LLM-driven fix loop set this so B3 doesn't hijack the path AND so
 * C6m provenance writes don't pollute the canonical principle JSONs.
 */
export async function recognize(args: RecognizeArgs): Promise<RecognizeResult> {
  if (process.env.PROVEKIT_DISABLE_RECOGNIZE === "1") {
    return { matched: false };
  }

  const logger = args.logger ?? createNoopLogger();
  const library = args.library ?? loadPrincipleLibrary();

  // --- 1. Resolve locus → file id ------------------------------------------
  const locusNode = args.db
    .select({
      sourceStart: nodes.sourceStart,
      sourceEnd: nodes.sourceEnd,
      sourceLine: nodes.sourceLine,
      fileId: nodes.fileId,
    })
    .from(nodes)
    .where(eq(nodes.id, args.locus.primaryNode))
    .get();

  if (!locusNode) {
    logger.detail(`[B3] locus.primaryNode ${args.locus.primaryNode} not in nodes table`);
    return { matched: false };
  }

  // --- 2. Lazy-populate principleMatches for the file -----------------------
  ensurePrincipleMatchesPopulated(args.db, locusNode.fileId, library.dir, logger);

  // --- 3. Find intersecting matches (exact + span-containment) -------------
  // Exact: rootMatchNode === locus.primaryNode.
  let candidates = args.db
    .select({
      id: principleMatches.id,
      principleName: principleMatches.principleName,
      rootMatchNodeId: principleMatches.rootMatchNodeId,
    })
    .from(principleMatches)
    .where(eq(principleMatches.rootMatchNodeId, args.locus.primaryNode))
    .all();

  // Span-containment fallback: locus is inside a match's root span.
  if (candidates.length === 0) {
    const containing = args.db
      .select({
        id: principleMatches.id,
        principleName: principleMatches.principleName,
        rootMatchNodeId: principleMatches.rootMatchNodeId,
      })
      .from(principleMatches)
      .innerJoin(nodes, eq(nodes.id, principleMatches.rootMatchNodeId))
      .where(
        and(
          eq(nodes.fileId, locusNode.fileId),
          lte(nodes.sourceStart, locusNode.sourceStart),
          gte(nodes.sourceEnd, locusNode.sourceEnd),
        ),
      )
      .all();
    candidates = containing;
  }

  // Same-line fallback: locate() can pick a sibling token (e.g. a SemicolonToken
  // adjacent to the matched BinaryExpression). Both sit on the same source line
  // of the same function. Treat that as a recognition hit — the locus and the
  // match are pointing at the same logical bug site, even though their byte
  // ranges don't intersect.
  if (candidates.length === 0) {
    const sameLine = args.db
      .select({
        id: principleMatches.id,
        principleName: principleMatches.principleName,
        rootMatchNodeId: principleMatches.rootMatchNodeId,
      })
      .from(principleMatches)
      .innerJoin(nodes, eq(nodes.id, principleMatches.rootMatchNodeId))
      .where(
        and(
          eq(nodes.fileId, locusNode.fileId),
          eq(nodes.sourceLine, locusNode.sourceLine),
        ),
      )
      .all();
    candidates = sameLine;
  }

  if (candidates.length === 0) {
    return { matched: false };
  }

  // --- 4. Filter candidates whose principle has BOTH templates --------------
  const ready: Array<{ matchId: number; rootMatchNodeId: string; principle: LibraryPrinciple }> = [];

  for (const cand of candidates) {
    const principle = library.byId.get(cand.principleName);
    if (!principle) {
      logger.detail(`[B3] match for principle '${cand.principleName}' but no JSON in library`);
      continue;
    }
    if (!principle.fixTemplate || !principle.testTemplate) {
      const missing =
        !principle.fixTemplate && !principle.testTemplate
          ? "fixTemplate AND testTemplate"
          : !principle.fixTemplate
            ? "fixTemplate"
            : "testTemplate";
      logger.detail(
        `[B3] WARN: principle '${principle.id}' matched at locus but lacks ${missing}; ` +
          `falling back to LLM mode`,
      );
      continue;
    }
    ready.push({ matchId: cand.id, rootMatchNodeId: cand.rootMatchNodeId, principle });
  }

  if (ready.length === 0) {
    return { matched: false };
  }

  // --- 5. Pick highest-confidence ready match ------------------------------
  ready.sort((a, b) => confidenceScore(b.principle) - confidenceScore(a.principle));
  const winner = ready[0]!;

  // --- 6. Resolve bindings ------------------------------------------------
  const captureRows = args.db
    .select({
      captureName: principleMatchCaptures.captureName,
      capturedNodeId: principleMatchCaptures.capturedNodeId,
    })
    .from(principleMatchCaptures)
    .where(eq(principleMatchCaptures.matchId, winner.matchId))
    .all();

  const bindings: Record<string, string> = {};
  for (const r of captureRows) {
    bindings[r.captureName] = r.capturedNodeId;
  }

  logger.detail(
    `[B3] recognized principle '${winner.principle.id}' at ${args.locus.primaryNode} ` +
      `(confidence=${winner.principle.confidence ?? "unspecified"}, ${captureRows.length} captures)`,
  );

  return {
    matched: true,
    principleId: winner.principle.id,
    bugClassId: winner.principle.bug_class_id,
    bindings,
    principle: winner.principle,
    matchId: winner.matchId,
    rootMatchNodeId: winner.rootMatchNodeId,
  };
}
