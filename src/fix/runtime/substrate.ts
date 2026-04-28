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
 * Resolve a (filePath, line) pair to a substrate node id. Returns the
 * smallest node whose span covers the given line — typically a
 * statement, expression, or call site at that line. Used by the path
 * enumerator to translate the invariant store's file+line callsite
 * reference into a node id the data_flow graph can be walked from.
 *
 * Returns null when no node matches (file not in substrate, line out
 * of range, etc.). Caller surfaces that as a substrate-staleness
 * decay signal.
 */
export function resolveCallsiteNodeId(
  db: Db,
  filePath: string,
  line: number,
): string | null {
  // Translate the file path through the substrate's `files` table to a
  // file id, then find the node at the given line. Substrate node spans
  // are recorded by character (sourceStart, sourceEnd) plus the starting
  // line; we approximate "node covers this line" by selecting nodes
  // whose recorded sourceLine equals the target. v1 best-effort —
  // substrate enrichment in step 4 will tighten the geometry.
  const fileRow = db
    .select({ id: files.id })
    .from(files)
    .where(eq(files.path, filePath))
    .get();
  if (!fileRow) return null;

  const candidates = db
    .select({
      id: nodes.id,
      sourceStart: nodes.sourceStart,
      sourceEnd: nodes.sourceEnd,
      sourceLine: nodes.sourceLine,
    })
    .from(nodes)
    .where(eq(nodes.fileId, fileRow.id))
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
