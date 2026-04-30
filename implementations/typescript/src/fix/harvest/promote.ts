/**
 * Phase 4 (lite): staging → library promotion.
 *
 * Reads a staged HarvestedPrinciple JSON from .provekit/harvest/staging/,
 * runs validateStagedPrinciple against the source candidate + cohort, and
 * — if validation passes — writes the principle into the source-controlled
 * library at .provekit/principles/<name>.dsl + <name>.json.
 *
 * If validation fails, the staged record is updated with a quarantine
 * marker carrying the failure reason. Quarantined entries stay in staging
 * for human review.
 */

import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "fs";
import { join } from "path";
import type { LibraryPrinciple } from "../types.js";
import type { HarvestCandidate } from "./extractBugs.js";
import { validateStagedPrinciple, type ValidationResult } from "./validate.js";
import { resolveWritePartition } from "../../principleEnumeration.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * Shape of a staged JSON record produced by harvest-discover.ts. Only the
 * fields this module reads are listed; extra fields (synthesizedInvariant,
 * invariantRaw, etc.) are preserved on rewrite.
 */
export interface StagedRecord {
  candidate: {
    source: HarvestCandidate["source"];
    upstreamFixMessage: string;
    stats: HarvestCandidate["stats"];
    diff: string;
  };
  outcome: { kind: string; [k: string]: unknown };
  principles: Array<{
    kind: string;
    name: string;
    bugClassId: string;
    dslSource?: string;
  }>;
  /** Set by promote() when validation runs. */
  validation?: ValidationResult & { promoted: boolean; principleName: string };
  [k: string]: unknown;
}

export interface PromoteOptions {
  /** Path to the staged JSON record file. */
  stagedPath: string;
  /** The full HarvestCandidate the staged principle was distilled from. */
  source: HarvestCandidate;
  /** Other candidates to use as the negative cohort. */
  cohort: HarvestCandidate[];
  /** Library directory; defaults to project's .provekit/principles/. */
  principlesDir?: string;
  /** Maximum cohort match rate before validation fails. Default 0.3. */
  maxCohortMatchRate?: number;
  /** Optional parent for the validate() scratch dirs. */
  scratchParent?: string;
}

export interface PromoteResult {
  /** Number of principles in this staged record that were promoted. */
  promoted: number;
  /** Number quarantined (validation failed). */
  quarantined: number;
  /** Per-principle outcomes for log/report consumption. */
  perPrinciple: Array<{ name: string; promoted: boolean; reason: string }>;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Promote (or quarantine) every principle in a staged record. The record
 * file is rewritten in place with the validation outcome attached so the
 * staging directory carries a durable audit trail.
 */
export function promoteStagedRecord(opts: PromoteOptions): PromoteResult {
  const { stagedPath, source, cohort } = opts;
  const principlesDir = opts.principlesDir ?? defaultPrinciplesDir();

  const raw = readFileSync(stagedPath, "utf-8");
  const record = JSON.parse(raw) as StagedRecord;

  const perPrinciple: PromoteResult["perPrinciple"] = [];
  let promoted = 0;
  let quarantined = 0;

  for (const principle of record.principles) {
    const dslSource = principle.dslSource;
    if (!dslSource || dslSource.trim().length === 0) {
      perPrinciple.push({ name: principle.name, promoted: false, reason: "no DSL source on staged record" });
      quarantined++;
      continue;
    }

    const validation = validateStagedPrinciple({
      dslSource,
      source,
      cohort,
      maxCohortMatchRate: opts.maxCohortMatchRate,
      scratchParent: opts.scratchParent,
    });

    if (!validation.passed) {
      perPrinciple.push({ name: principle.name, promoted: false, reason: validation.reason });
      quarantined++;
      continue;
    }

    // Validation passed — write DSL + JSON to the library partition
    // (task #134). Promoted principles default to universal/ unless a
    // language tag is present on the principle metadata. Today the
    // staged record carries no language tag, so universal/ is the
    // conservative default; once harvest captures the source-corpus
    // language we'll route per-language.
    const partitionDir = resolveWritePartition(
      principlesDir,
      (principle as { language?: string }).language as any,
    );
    mkdirSync(partitionDir, { recursive: true });
    const dslPath = join(partitionDir, `${principle.name}.dsl`);
    writeFileSync(dslPath, dslSource, "utf-8");

    const libraryEntry: LibraryPrinciple = {
      id: principle.name,
      bug_class_id: principle.bugClassId,
      name: principle.name,
      provenance: [{
        source: "harvest",
        projectId: source.source.project,
        bugId: source.source.bugId,
        timestamp: new Date().toISOString(),
      }],
      confidence: "medium", // harvested + validated; humans can promote to "high" later
    };
    const jsonPath = join(partitionDir, `${principle.name}.json`);
    // If the file already exists (e.g. promoted previously), preserve any
    // additional fields a maintainer added by hand.
    let merged: LibraryPrinciple = libraryEntry;
    if (existsSync(jsonPath)) {
      try {
        const existing = JSON.parse(readFileSync(jsonPath, "utf-8")) as LibraryPrinciple;
        merged = { ...existing, ...libraryEntry, provenance: mergeProvenance(existing.provenance, libraryEntry.provenance!) };
      } catch { /* fall back to fresh write */ }
    }
    writeFileSync(jsonPath, JSON.stringify(merged, null, 2) + "\n", "utf-8");

    perPrinciple.push({ name: principle.name, promoted: true, reason: validation.reason });
    promoted++;
  }

  // Rewrite the staged record with a validation summary attached.
  const recordWithAudit = {
    ...record,
    validation: {
      perPrinciple,
      promoted,
      quarantined,
      timestamp: new Date().toISOString(),
    },
  };
  writeFileSync(stagedPath, JSON.stringify(recordWithAudit, null, 2) + "\n", "utf-8");

  return { promoted, quarantined, perPrinciple };
}

/**
 * Walk every staged record under `stagingDir`, promote each. Bulk entry
 * point used by the harvest CLI.
 */
export function promoteAllStaged(opts: {
  stagingDir: string;
  candidatesById: Map<string, HarvestCandidate>;
  principlesDir?: string;
  cohortSize?: number;
  maxCohortMatchRate?: number;
  scratchParent?: string;
}): { totalRecords: number; totalPromoted: number; totalQuarantined: number } {
  const { stagingDir, candidatesById } = opts;
  const cohortSize = opts.cohortSize ?? 10;
  if (!existsSync(stagingDir)) {
    return { totalRecords: 0, totalPromoted: 0, totalQuarantined: 0 };
  }

  const allCandidates = Array.from(candidatesById.values());
  let totalRecords = 0;
  let totalPromoted = 0;
  let totalQuarantined = 0;

  for (const file of readdirSync(stagingDir).filter((f) => f.endsWith(".json"))) {
    totalRecords++;
    const stagedPath = join(stagingDir, file);
    let record: StagedRecord;
    try {
      record = JSON.parse(readFileSync(stagedPath, "utf-8")) as StagedRecord;
    } catch {
      continue;
    }
    const sourceKey = `${record.candidate.source.project}-${record.candidate.source.bugId}`;
    const source = candidatesById.get(sourceKey);
    if (!source) continue;

    const cohort = pickCohort(allCandidates, source, cohortSize);
    const r = promoteStagedRecord({
      stagedPath,
      source,
      cohort,
      principlesDir: opts.principlesDir,
      maxCohortMatchRate: opts.maxCohortMatchRate,
      scratchParent: opts.scratchParent,
    });
    totalPromoted += r.promoted;
    totalQuarantined += r.quarantined;
  }

  return { totalRecords, totalPromoted, totalQuarantined };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function pickCohort(all: HarvestCandidate[], exclude: HarvestCandidate, size: number): HarvestCandidate[] {
  const excludeKey = `${exclude.source.project}-${exclude.source.bugId}`;
  const filtered = all.filter((c) => `${c.source.project}-${c.source.bugId}` !== excludeKey);
  // Deterministic selection: take the first `size` after a stable sort.
  filtered.sort((a, b) => `${a.source.project}-${a.source.bugId}`.localeCompare(`${b.source.project}-${b.source.bugId}`));
  return filtered.slice(0, size);
}

function mergeProvenance(
  existing: LibraryPrinciple["provenance"] | undefined,
  added: LibraryPrinciple["provenance"],
): LibraryPrinciple["provenance"] {
  const norm = (p: LibraryPrinciple["provenance"] | undefined) =>
    p === undefined ? [] : Array.isArray(p) ? p : [p];
  const out = [...norm(existing)];
  for (const a of norm(added)) {
    const dup = out.some((e) =>
      e.source === a.source && e.projectId === a.projectId && e.bugId === a.bugId,
    );
    if (!dup) out.push(a);
  }
  return out;
}

function defaultPrinciplesDir(): string {
  // src/fix/harvest/promote.ts → projectRoot/.provekit/principles
  // Importing url's fileURLToPath would cycle the dependency; use a
  // process.cwd-based relative resolution instead, with a fallback.
  return join(process.cwd(), ".provekit", "principles");
}
