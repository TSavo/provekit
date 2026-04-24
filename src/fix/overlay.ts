/**
 * C2: Overlay helpers — apply patches, re-index, and close scratch worktrees.
 *
 * The OverlayHandle is created by openOverlay (stages/openOverlay.ts).
 * These helpers operate on that handle without ever touching the main DB.
 */

import { writeFileSync, unlinkSync, existsSync } from "fs";
import { join } from "path";
import { execFileSync } from "child_process";
import { buildSASTForFile, reindexFile } from "../sast/builder.js";
import type { OverlayHandle, CodePatch } from "./types.js";

/**
 * Apply a whole-file patch to the overlay's scratch worktree.
 *
 * Writes patch.newContent to <worktreePath>/<patch.file> and records the
 * relative path in overlay.modifiedFiles. Never touches the original repo.
 *
 * Throws if the overlay is closed.
 */
export function applyPatchToOverlay(overlay: OverlayHandle, patch: CodePatch): void {
  if (overlay.closed) {
    throw new Error("applyPatchToOverlay: overlay is already closed");
  }
  const absPath = join(overlay.worktreePath, patch.file);
  writeFileSync(absPath, patch.newContent, "utf8");
  overlay.modifiedFiles.add(patch.file);
}

/**
 * Re-index files in the overlay's scratch SAST DB.
 *
 * If `files` is omitted, re-indexes all files in overlay.modifiedFiles.
 * Uses reindexFile (force rebuild) so content-hash short-circuit doesn't
 * prevent freshly-written patches from being picked up.
 *
 * Throws if the overlay is closed.
 */
export async function reindexOverlay(overlay: OverlayHandle, files?: string[]): Promise<void> {
  if (overlay.closed) {
    throw new Error("reindexOverlay: overlay is already closed");
  }
  const targets = files ?? Array.from(overlay.modifiedFiles);
  for (const rel of targets) {
    const absPath = join(overlay.worktreePath, rel);
    reindexFile(overlay.sastDb, absPath);
  }
}

/**
 * Close the overlay: flush + close the scratch DB, remove the git worktree,
 * and delete the scratch DB file. Sets overlay.closed = true.
 *
 * If overlay is already closed, this is a no-op.
 */
export async function closeOverlay(overlay: OverlayHandle): Promise<void> {
  if (overlay.closed) {
    return;
  }

  // Mark closed first so partial failures don't leave it in a re-usable state.
  overlay.closed = true;

  // Close the Drizzle/better-sqlite3 handle.
  try {
    overlay.sastDb.$client.close();
  } catch {
    // Ignore — already closed or never opened cleanly.
  }

  // Remove the git worktree. We need the original repo root (parent of worktreePath).
  // The worktree was registered under the original repo so we remove it there.
  let worktreeRemoved = false;
  try {
    // Determine the repo root that owns this worktree. Because we added it with
    // --detach from that root, the original root knows about it.
    // We recover the repo root by reading the worktree's .git file (a gitfile
    // pointing at the common git dir) — but the simplest approach is to call
    // `git worktree remove` with the worktree path itself; git resolves the
    // common git dir internally.
    execFileSync("git", ["worktree", "remove", "--force", overlay.worktreePath], {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    worktreeRemoved = true;
  } catch {
    // worktree remove failed (e.g., directory was partially created before
    // `git worktree add` succeeded). Fall back to fs.rmSync.
  }

  if (!worktreeRemoved && existsSync(overlay.worktreePath)) {
    const { rmSync } = await import("fs");
    rmSync(overlay.worktreePath, { recursive: true, force: true });
  }

  // Delete the scratch DB file if still present (worktree remove may have
  // already cleaned it if it lived inside the worktree).
  if (existsSync(overlay.sastDbPath)) {
    try {
      unlinkSync(overlay.sastDbPath);
    } catch {
      // Best effort.
    }
  }
}
