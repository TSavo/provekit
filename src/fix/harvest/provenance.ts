/**
 * Append-only provenance writer for the principle library.
 *
 * When recognition mode says "principle X already covers candidate Y," the
 * harvest pipeline calls this to push `{source: "harvest", projectId: ...,
 * bugId: ...}` onto principle X's `provenance` array. The library entry's
 * count of grounding bugs grows; the principle DSL itself is unchanged.
 *
 * Pure I/O — no LLM, no SAST, no side effects beyond writing the JSON.
 * Designed to be called in batch by the harvest pipeline AFTER all
 * recognitions for a run have completed, so library mutations happen at
 * known points (not interleaved with read paths).
 */

import { existsSync, readFileSync, readdirSync, writeFileSync } from "fs";
import { join } from "path";
import type { BugProvenance, LibraryPrinciple } from "../types.js";

export interface HarvestProvenanceEntry {
  /** Principle id (matches the JSON filename minus .json). */
  principleId: string;
  /** Project the matched bug came from (e.g. "express"). */
  projectId: string;
  /** Bug ID within the project (numeric string). */
  bugId: string;
  /** ISO timestamp; defaults to now. */
  timestamp?: string;
}

export interface AppendResult {
  /** Number of provenance entries written. */
  appended: number;
  /** Principles that were not found in the library directory. */
  missingPrinciples: string[];
  /** Entries skipped because the same {projectId, bugId} pair was already in provenance. */
  duplicates: number;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Append harvest provenance entries to their principle JSON files.
 *
 * Idempotent: if the same {projectId, bugId} pair is already present in a
 * principle's provenance array, it is NOT added again. This lets a harvest
 * run be re-run safely without inflating provenance counts.
 *
 * The on-disk shape of `provenance` may be missing, an object, or an array.
 * This function normalizes to an array on write.
 */
export function appendHarvestProvenance(
  entries: HarvestProvenanceEntry[],
  principlesDir: string,
): AppendResult {
  if (!existsSync(principlesDir)) {
    return { appended: 0, missingPrinciples: entries.map((e) => e.principleId), duplicates: 0 };
  }

  // Cache loaded JSONs so multiple entries for the same principle share a
  // single read + write cycle.
  const cache = new Map<string, LibraryPrinciple>();
  const principleIdToFile = buildPrincipleIdIndex(principlesDir);

  const missingPrinciples: string[] = [];
  let appended = 0;
  let duplicates = 0;

  for (const entry of entries) {
    const path = principleIdToFile.get(entry.principleId);
    if (!path) {
      missingPrinciples.push(entry.principleId);
      continue;
    }

    let principle = cache.get(path);
    if (!principle) {
      try {
        principle = JSON.parse(readFileSync(path, "utf-8")) as LibraryPrinciple;
      } catch {
        missingPrinciples.push(entry.principleId);
        continue;
      }
      cache.set(path, principle);
    }

    const existing = normalizeProvenance(principle.provenance);
    const newEntry: BugProvenance = {
      source: "harvest",
      projectId: entry.projectId,
      bugId: entry.bugId,
      timestamp: entry.timestamp ?? new Date().toISOString(),
    };

    if (provenanceContains(existing, newEntry)) {
      duplicates++;
      continue;
    }

    existing.push(newEntry);
    principle.provenance = existing;
    appended++;
  }

  // Write each modified principle once.
  for (const [path, principle] of cache) {
    writeFileSync(path, JSON.stringify(principle, null, 2) + "\n", "utf-8");
  }

  return { appended, missingPrinciples, duplicates };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Index principle id → JSON file path. The id is the value of the JSON's
 * `id` field, which conventionally matches the filename stem; we read each
 * file once to be tolerant of mismatches.
 */
function buildPrincipleIdIndex(principlesDir: string): Map<string, string> {
  const out = new Map<string, string>();
  const files = readdirSync(principlesDir).filter((f) => f.endsWith(".json"));
  for (const file of files) {
    const path = join(principlesDir, file);
    try {
      const parsed = JSON.parse(readFileSync(path, "utf-8")) as LibraryPrinciple;
      if (typeof parsed.id === "string" && parsed.id.length > 0) {
        out.set(parsed.id, path);
      }
    } catch {
      // Skip unreadable / non-JSON entries silently — the harvest pipeline
      // logs those upstream when it tried to evaluate the matching DSL.
    }
  }
  return out;
}

function normalizeProvenance(p: BugProvenance | BugProvenance[] | undefined): BugProvenance[] {
  if (p === undefined) return [];
  if (Array.isArray(p)) return [...p];
  return [p];
}

function provenanceContains(arr: BugProvenance[], target: BugProvenance): boolean {
  for (const existing of arr) {
    if (
      existing.source === target.source &&
      existing.projectId === target.projectId &&
      existing.bugId === target.bugId
    ) {
      return true;
    }
  }
  return false;
}
