/**
 * Phase 2-D: harvest-time pre/post AST diff classification (hard-bug 1 Day 2).
 *
 * For each HarvestCandidate, walks every file that appears in both
 * `buggyFiles` and `fixedFiles`, runs `computeFileDiff`, and persists the
 * results to `pre_post_diff`. Files that exist only on one side are skipped
 * (this stage classifies node-level changes within a file; whole-file
 * adds/removes are a separate concern surfaced by extractBugs.stats).
 *
 * Day 3 will add a DSL relation that queries this table by post-side
 * coordinates to expose pre/post change kind to principle authors. The
 * query pattern is `(context, file_path, post_start, post_kind) → row`,
 * which the schema indexes directly.
 *
 * Idempotent: caller is responsible for deleting prior rows for the same
 * `context` before re-running. We don't auto-clear because the caller may
 * want to compare runs.
 */

import { eq, sql } from "drizzle-orm";
import type { Db } from "../../db/index.js";
import { prePostDiff, diffContextActive } from "../../db/schema/preDiff.js";
import { computeFileDiff, type DiffEntry } from "../../sast/diff.js";
import type { HarvestCandidate } from "./extractBugs.js";

export interface RecordCandidateDiffResult {
  filesProcessed: number;
  rowsInserted: number;
  perFile: Array<{
    filePath: string;
    summary: { unchanged: number; modified: number; added: number; deleted: number };
  }>;
}

/**
 * Compute and persist the per-node diff for every file present in both
 * sides of `candidate`. Returns counts for caller-side accounting.
 */
export function recordCandidateDiff(
  db: Db,
  candidate: HarvestCandidate,
): RecordCandidateDiffResult {
  const context = `harvest:${candidate.source.project}:${candidate.source.bugId}`;
  let filesProcessed = 0;
  let rowsInserted = 0;
  const perFile: RecordCandidateDiffResult["perFile"] = [];

  for (const filePath of Object.keys(candidate.buggyFiles)) {
    const preSrc = candidate.buggyFiles[filePath]!;
    const postSrc = candidate.fixedFiles[filePath];
    if (postSrc === undefined) continue; // file deleted in fix; no per-node diff

    const entries = computeFileDiff(preSrc, postSrc, filePath);
    const summary = { unchanged: 0, modified: 0, added: 0, deleted: 0 };
    for (const e of entries) summary[e.changeKind] += 1;
    perFile.push({ filePath, summary });

    const rows = entries.map((e) => entryToRow(context, filePath, e));
    if (rows.length > 0) {
      // Batch insert: pre_post_diff has 21 columns, sqlite caps bound
      // params at SQLITE_MAX_VARIABLE_NUMBER (32766 since 3.32; was 999).
      // 21 cols * 100 rows = 2100 params per batch leaves headroom for both.
      const BATCH = 100;
      for (let i = 0; i < rows.length; i += BATCH) {
        db.insert(prePostDiff).values(rows.slice(i, i + BATCH)).run();
      }
      rowsInserted += rows.length;
    }
    filesProcessed += 1;
  }

  return { filesProcessed, rowsInserted, perFile };
}

/**
 * Delete all rows for a given context. Use before re-running `recordCandidateDiff`
 * on the same candidate so a stale run doesn't double-count.
 */
export function clearCandidateDiff(
  db: Db,
  project: string,
  bugId: string,
): number {
  const context = `harvest:${project}:${bugId}`;
  const result = db.delete(prePostDiff).where(eq(prePostDiff.context, context)).run();
  return Number(result.changes ?? 0);
}

/**
 * Tell the DSL relation layer which diff context is currently in scope.
 * Must be called BEFORE evaluating a principle that uses diff-aware
 * relations (`was_replaced_by_addition`, etc.) — otherwise those
 * relations correctly report false (no context = no diff to compare).
 */
export function setActiveDiffContext(db: Db, context: string): void {
  // Single-row replace: delete then insert (sqlite has no convenient
  // UPSERT in drizzle for a fixed-PK case at our version level).
  db.delete(diffContextActive).run();
  db.insert(diffContextActive).values({ k: "active", context }).run();
}

/** Clear the active diff context. Diff-aware relations now report false. */
export function clearActiveDiffContext(db: Db): void {
  db.delete(diffContextActive).run();
}

/** Convenience: set active context to the candidate's harvest tag. */
export function setActiveCandidate(
  db: Db,
  project: string,
  bugId: string,
): void {
  setActiveDiffContext(db, `harvest:${project}:${bugId}`);
}

function entryToRow(context: string, filePath: string, e: DiffEntry) {
  return {
    context,
    filePath,
    changeKind: e.changeKind,

    preFingerprint: e.pre?.fingerprint ?? null,
    preParentFingerprint: e.pre?.parentFingerprint ?? null,
    preOrdinal: e.pre?.ordinal ?? null,
    preKind: e.pre?.kindName ?? null,
    preLine: e.pre?.line ?? null,
    preCol: e.pre?.column ?? null,
    preStart: e.pre?.start ?? null,
    preEnd: e.pre?.end ?? null,
    preTextPreview: e.pre?.textPreview ?? null,

    postFingerprint: e.post?.fingerprint ?? null,
    postParentFingerprint: e.post?.parentFingerprint ?? null,
    postOrdinal: e.post?.ordinal ?? null,
    postKind: e.post?.kindName ?? null,
    postLine: e.post?.line ?? null,
    postCol: e.post?.column ?? null,
    postStart: e.post?.start ?? null,
    postEnd: e.post?.end ?? null,
    postTextPreview: e.post?.textPreview ?? null,
  };
}
