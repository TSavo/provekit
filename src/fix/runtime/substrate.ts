/**
 * Thin wrapper around the substrate (.provekit/provekit.db) for the
 * standing-runtime's read-side operations.
 *
 * The fix loop already opens this DB at orchestration time. The verify
 * CLI / path enumerator need an independent open path because they run
 * outside the fix loop's lifecycle.
 */

import { existsSync } from "fs";
import { join } from "path";
import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import { eq } from "drizzle-orm";
import type { Db } from "../../db/index.js";
import { files, nodes } from "../../sast/schema/nodes.js";

/**
 * Open the project's substrate database for read. Returns null if the
 * substrate hasn't been built yet (the user hasn't run `provekit
 * analyze` / hasn't shipped a fix loop yet). Caller decides what to do.
 */
export function openSubstrateDb(projectRoot: string): Db | null {
  const dbPath = join(projectRoot, ".provekit", "provekit.db");
  if (!existsSync(dbPath)) return null;
  const sqlite = new Database(dbPath, { readonly: true, fileMustExist: true });
  return drizzle(sqlite) as unknown as Db;
}

/**
 * Direct line lookup. Returns the smallest-span node at the given line, or
 * null. Used by both the canonical resolve path and the recovery path that
 * recomputes a target line from a function's current startLine + offset.
 */
function resolveByLine(db: Db, fileId: number, line: number): string | null {
  const candidates = db
    .select({
      id: nodes.id,
      sourceStart: nodes.sourceStart,
      sourceEnd: nodes.sourceEnd,
      sourceLine: nodes.sourceLine,
    })
    .from(nodes)
    .where(eq(nodes.fileId, fileId))
    .all();
  let best: { id: string; span: number } | null = null;
  for (const c of candidates) {
    if (c.sourceLine !== line) continue;
    const span = c.sourceEnd - c.sourceStart;
    if (best === null || span < best.span) {
      best = { id: c.id, span };
    }
  }
  return best?.id ?? null;
}

/**
 * Find the function-shaped node in `filePath` whose `subtreeHash` matches
 * `functionHash`. Returns its current sourceLine (or null if no such
 * function exists in the substrate). Used by the resolver's recovery path
 * when the recorded line no longer hits a node directly.
 */
export function findFunctionLineByHash(
  db: Db,
  filePath: string,
  functionHash: string,
): number | null {
  const fileRow = db
    .select({ id: files.id })
    .from(files)
    .where(eq(files.path, filePath))
    .get();
  if (!fileRow) return null;
  const candidates = db
    .select({
      id: nodes.id,
      sourceLine: nodes.sourceLine,
      subtreeHash: nodes.subtreeHash,
      kind: nodes.kind,
    })
    .from(nodes)
    .where(eq(nodes.fileId, fileRow.id))
    .all();
  for (const c of candidates) {
    if (c.subtreeHash === functionHash) return c.sourceLine;
  }
  return null;
}

/**
 * Resolve a callsite reference to a substrate node id, with self-healing.
 *
 * Four-way state machine:
 *   1. Line directly resolves → return node id (HOLDS)
 *   2. Line missed; functionHash + functionOffset recover the new line →
 *      return node id at the recovered line (HOLDS, self-heal)
 *   3. functionHash present but no node has that hash anymore → null
 *      (DECAYED — content changed; semantic decay)
 *   4. functionHash absent and direct line missed → null (GONE under the
 *      legacy line-only contract; caller can decide retire vs reauthor)
 *
 * `recoveryHints` is optional; when omitted the resolver falls back to
 * line-only behavior (case 1 only; misses report null = decay). New
 * writers should pass functionHash + functionOffset so cases 2-4 become
 * distinguishable upstream.
 */
export function resolveCallsiteNodeId(
  db: Db,
  filePath: string,
  line: number,
  recoveryHints?: { functionHash?: string | null; functionOffset?: number | null },
): string | null {
  const fileRow = db
    .select({ id: files.id })
    .from(files)
    .where(eq(files.path, filePath))
    .get();
  if (!fileRow) return null;

  const direct = resolveByLine(db, fileRow.id, line);
  if (direct) return direct;

  const fh = recoveryHints?.functionHash;
  const fo = recoveryHints?.functionOffset;
  if (fh != null && fo != null) {
    const fnLine = findFunctionLineByHash(db, filePath, fh);
    if (fnLine !== null) {
      const recoveredLine = fnLine + fo;
      const recovered = resolveByLine(db, fileRow.id, recoveredLine);
      if (recovered) return recovered;
    }
  }
  return null;
}
