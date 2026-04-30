/**
 * Phase 4 (lite): cross-corpus validation for staged harvested principles.
 *
 * After Phase 2-B (discovery) produces a candidate principle, run a cheap
 * positive + negative check before promoting it to the library:
 *
 *   - Positive: the principle MUST match the buggy snapshot it was
 *     harvested from, at its diff locus. If it doesn't fire here, the
 *     discovery output is a parser-valid principle that doesn't actually
 *     describe the bug it was distilled from.
 *
 *   - Negative: across a cohort of OTHER buggy candidates' snapshots, the
 *     principle must NOT match too broadly. The threshold is tunable; the
 *     default is ≤ 30% false-positive rate, mirroring the production
 *     adversarial validator's discrimination requirement.
 *
 * This is harvest's analogue of oracle #6 (production adversarial validation),
 * but uses real BugsJS bugs as the negative cohort instead of LLM-generated
 * fixtures. The cohort isn't a perfect oracle — some "false positives" may
 * be true bugs of the same class — but it's a meaningful filter against
 * principles that fire on every || or every / in the codebase.
 *
 * No LLM. Pure SAST + DSL evaluation.
 */

import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { dirname, join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { fileURLToPath } from "url";
import { openDb } from "../../db/index.js";
import { buildSASTForFile } from "../../sast/builder.js";
import { evaluatePrinciple } from "../../dsl/evaluator.js";
import { principleMatches } from "../../db/schema/principleMatches.js";
import { files } from "../../sast/schema/nodes.js";
import { eq, inArray } from "drizzle-orm";
import { parseDiffDirtyLines } from "./recognize.js";
import type { HarvestCandidate } from "./extractBugs.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ValidationResult {
  /** Did the principle match the candidate it was harvested from at its locus? */
  positivePass: boolean;
  /** Number of cohort candidates the principle matched (false positives + true positives). */
  cohortMatchCount: number;
  /** Total cohort size. */
  cohortSize: number;
  /** Cohort match rate as a fraction in [0, 1]. */
  cohortMatchRate: number;
  /** True if positivePass AND cohortMatchRate <= maxCohortMatchRate. */
  passed: boolean;
  /** Reason summary for logs/staging metadata. */
  reason: string;
}

export interface ValidateOptions {
  /** The DSL source for the principle being validated. */
  dslSource: string;
  /** The candidate this principle was harvested from. Provides positive test. */
  source: HarvestCandidate;
  /** Other candidates whose buggy snapshots are used as the negative cohort. */
  cohort: HarvestCandidate[];
  /** Maximum acceptable cohort match rate. Default 0.3 (30%). */
  maxCohortMatchRate?: number;
  /** Optional parent for the scratch dir. */
  scratchParent?: string;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function validateStagedPrinciple(opts: ValidateOptions): ValidationResult {
  const maxRate = opts.maxCohortMatchRate ?? 0.3;
  const scratchParent = opts.scratchParent ?? tmpdir();

  // Positive check: does the principle match the source candidate's buggy
  // file at its diff locus?
  const positivePass = principleMatchesAtLocus(opts.dslSource, opts.source, scratchParent);

  // Negative check: how many cohort candidates does the principle match
  // (anywhere in their buggy production files)?
  let cohortMatchCount = 0;
  for (const c of opts.cohort) {
    if (principleMatchesAnywhere(opts.dslSource, c, scratchParent)) {
      cohortMatchCount++;
    }
  }
  const cohortSize = opts.cohort.length;
  const cohortMatchRate = cohortSize === 0 ? 0 : cohortMatchCount / cohortSize;

  const passed = positivePass && cohortMatchRate <= maxRate;
  const reason = passed
    ? `positive=pass cohort=${cohortMatchCount}/${cohortSize} (${(cohortMatchRate * 100).toFixed(0)}%) <= ${(maxRate * 100).toFixed(0)}%`
    : !positivePass
      ? `positive FAIL — principle did not match its own source bug at the diff locus`
      : `cohort match rate ${(cohortMatchRate * 100).toFixed(0)}% > ${(maxRate * 100).toFixed(0)}% threshold (${cohortMatchCount}/${cohortSize})`;

  return { positivePass, cohortMatchCount, cohortSize, cohortMatchRate, passed, reason };
}

// ---------------------------------------------------------------------------
// Internal: per-candidate evaluation
// ---------------------------------------------------------------------------

function principleMatchesAtLocus(
  dslSource: string,
  candidate: HarvestCandidate,
  scratchParent: string,
): boolean {
  const dirtyByPath = parseDiffDirtyLines(candidate.diff);
  return runPrincipleAgainstCandidate(dslSource, candidate, scratchParent, dirtyByPath);
}

function principleMatchesAnywhere(
  dslSource: string,
  candidate: HarvestCandidate,
  scratchParent: string,
): boolean {
  // For cohort candidates we don't know their bug locus matters per-se —
  // we just want to know whether the harvested principle fires at all in
  // their buggy code. A pass-through "any match counts" check.
  return runPrincipleAgainstCandidate(dslSource, candidate, scratchParent, null);
}

function runPrincipleAgainstCandidate(
  dslSource: string,
  candidate: HarvestCandidate,
  scratchParent: string,
  dirtyByPath: Map<string, Array<[number, number]>> | null,
): boolean {
  const scratchDir = mkdtempSync(join(scratchParent, "provekit-harvest-validate-"));
  let db: ReturnType<typeof openDb> | null = null;
  try {
    // 1. Materialize buggy production files.
    const productionPaths: { abs: string; rel: string }[] = [];
    for (const [relPath, content] of Object.entries(candidate.buggyFiles)) {
      if (isTestPath(relPath)) continue;
      const abs = join(scratchDir, "src", relPath);
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, content, "utf-8");
      productionPaths.push({ abs, rel: relPath });
    }
    if (productionPaths.length === 0) return false;

    // 2. Open scratch DB + migrations + SAST build.
    db = openDb(join(scratchDir, "scratch.db"));
    migrate(db, { migrationsFolder: resolveMigrationsDir() });
    for (const p of productionPaths) {
      try { buildSASTForFile(db, p.abs); } catch { /* per-file errors are non-fatal */ }
    }

    // 3. Evaluate the principle.
    try {
      evaluatePrinciple(db, dslSource);
    } catch {
      return false;
    }

    // 4. Read matches. If we have a dirty-line map, only count matches that
    //    fall within a dirty range; otherwise any match counts.
    const matchRows = db
      .select({
        fileId: principleMatches.fileId,
        rootNodeId: principleMatches.rootMatchNodeId,
      })
      .from(principleMatches)
      .all();
    if (matchRows.length === 0) return false;

    if (dirtyByPath === null) return true; // cohort path: any match suffices

    // Locus-constrained: walk file_ids back to paths, check source_line.
    const fileIds = Array.from(new Set(matchRows.map((r) => r.fileId)));
    const fileRows = db
      .select({ id: files.id, path: files.path })
      .from(files)
      .where(inArray(files.id, fileIds))
      .all();
    const pathById = new Map<number, string>();
    for (const r of fileRows) pathById.set(r.id, r.path);

    for (const m of matchRows) {
      const absPath = pathById.get(m.fileId);
      if (!absPath) continue;
      const rel = relativeToScratchSrc(absPath, scratchDir);
      if (rel === null) continue;
      const dirty = dirtyByPath.get(rel);
      if (!dirty) continue;
      const line = lineForNode(db, m.rootNodeId);
      for (const [start, end] of dirty) {
        if (line >= start && line <= end) return true;
      }
    }
    return false;
  } finally {
    try { if (db) db.$client.close(); } catch { /* ignore */ }
    try { rmSync(scratchDir, { recursive: true, force: true }); } catch { /* ignore */ }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function isTestPath(p: string): boolean {
  const segments = p.split("/");
  for (const seg of segments) {
    if (seg === "test" || seg === "tests" || seg === "__tests__") return true;
  }
  return /\.(test|spec)\.[^/]+$/.test(p);
}

function relativeToScratchSrc(absPath: string, scratchDir: string): string | null {
  const prefix = join(scratchDir, "src") + "/";
  if (!absPath.startsWith(prefix)) return null;
  return absPath.slice(prefix.length);
}

function lineForNode(db: ReturnType<typeof openDb>, nodeId: string): number {
  const rows = db.$client.prepare(`SELECT source_line FROM nodes WHERE id = ?`).all(nodeId) as { source_line: number }[];
  return rows[0]?.source_line ?? 0;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function resolveMigrationsDir(): string {
  return join(__dirname, "..", "..", "..", "drizzle");
}
