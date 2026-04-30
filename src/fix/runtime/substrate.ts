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
 * function exists in the file). Used by the resolver's recovery path
 * when the recorded line no longer hits a node directly.
 *
 * Use `findFunctionByHashGlobal` instead when the function may have moved
 * to a different file.
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
 * Search the entire substrate for a function-shaped node whose
 * `subtreeHash` matches `functionHash`. Returns the file path and current
 * sourceLine when found, regardless of which file currently houses it.
 *
 * The file path on the original binding is a hint, not a constraint.
 * Functions move between files (extraction refactors, file splits,
 * organization changes) without changing meaning. The substrate's
 * subtreeHash index is global; search it globally so a moved function
 * stays bound.
 *
 * Returns null when no node has that hash anywhere in the substrate.
 */
export function findFunctionByHashGlobal(
  db: Db,
  functionHash: string,
): { filePath: string; sourceLine: number } | null {
  // Single join across nodes + files indexed by nodes.subtree_hash.
  const candidates = db
    .select({
      sourceLine: nodes.sourceLine,
      subtreeHash: nodes.subtreeHash,
      filePath: files.path,
    })
    .from(nodes)
    .innerJoin(files, eq(files.id, nodes.fileId))
    .all();
  for (const c of candidates) {
    if (c.subtreeHash === functionHash) {
      return { filePath: c.filePath, sourceLine: c.sourceLine };
    }
  }
  return null;
}

/**
 * Resolve a callsite reference to a substrate node id, with self-healing.
 *
 * Five-way state machine:
 *   1. Line directly resolves → return node id (HOLDS)
 *   2. Line missed; functionHash + functionOffset recover the new line in
 *      the SAME file → return recovered node id (HOLDS, self-heal)
 *   3. Same-file recovery missed; functionHash matches a node in a
 *      DIFFERENT file → return recovered node id (HOLDS, self-heal across
 *      file move)
 *   4. functionHash present but no node has that hash anywhere in the
 *      substrate → null (DECAYED — content changed; semantic decay,
 *      LLM re-eval workflow handles this)
 *   5. functionHash absent and direct line missed → null (legacy line-
 *      only invariant degrades the same way it did before this feature)
 *
 * `recoveryHints` is optional; when omitted the resolver falls back to
 * line-only behavior (case 1 only). New writers should pass
 * functionHash + functionOffset so cases 2-5 become distinguishable.
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

  // Case 1: same file, direct line hit.
  if (fileRow) {
    const direct = resolveByLine(db, fileRow.id, line);
    if (direct) return direct;
  }

  const fh = recoveryHints?.functionHash;
  const fo = recoveryHints?.functionOffset;
  if (fh != null && fo != null) {
    // Case 2: same file, recover via functionHash + offset.
    if (fileRow) {
      const fnLine = findFunctionLineByHash(db, filePath, fh);
      if (fnLine !== null) {
        const recovered = resolveByLine(db, fileRow.id, fnLine + fo);
        if (recovered) return recovered;
      }
    }

    // Case 3: function moved to a different file. Search the substrate
    // globally by hash; if found, recover at the new file's location.
    const moved = findFunctionByHashGlobal(db, fh);
    if (moved) {
      const movedFileRow = db
        .select({ id: files.id })
        .from(files)
        .where(eq(files.path, moved.filePath))
        .get();
      if (movedFileRow) {
        const recovered = resolveByLine(db, movedFileRow.id, moved.sourceLine + fo);
        if (recovered) return recovered;
      }
    }
  }

  // Case 4 (hash given but globally missing) and case 5 (no recovery
  // hints, line miss) both report null. Caller distinguishes them by
  // looking at the original recoveryHints + the substrate state.
  return null;
}
