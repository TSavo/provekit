/**
 * C2: openOverlay — create a scratch git worktree with its own SAST DB.
 *
 * The scratch worktree is a detached HEAD clone of the current HEAD of the
 * repo containing locus.file. It has its own SAST DB (never the main DB).
 * The overlay is torn down by closeOverlay() in overlay.ts.
 */

import { mkdtempSync, mkdirSync, cpSync, existsSync, rmSync, realpathSync } from "fs";
import { join, relative, dirname } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { buildSASTForFile } from "../../sast/builder.js";
import type { BugLocus, OverlayHandle } from "../types.js";
import type { Db } from "../../db/index.js";

export async function openOverlay(args: {
  locus: BugLocus;
  db: Db;
}): Promise<OverlayHandle> {
  const { locus } = args;

  // ------------------------------------------------------------------
  // Step 1: Resolve original repo root and HEAD ref.
  //         Do this BEFORE creating any scratch resources so that a
  //         non-git directory fails cleanly without leaving garbage.
  // ------------------------------------------------------------------
  const locusDir = dirname(locus.file);

  let repoRoot: string;
  let baseRef: string;

  try {
    repoRoot = execFileSync(
      "git",
      ["rev-parse", "--show-toplevel"],
      { cwd: locusDir, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
    ).trim();
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(
      `openOverlay: locus file is not inside a git repository (checked ${locusDir}): ${msg}`,
    );
  }

  try {
    baseRef = execFileSync(
      "git",
      ["rev-parse", "HEAD"],
      { cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
    ).trim();
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`openOverlay: could not resolve HEAD in ${repoRoot}: ${msg}`);
  }

  // ------------------------------------------------------------------
  // Step 2: Create scratch directory.
  // ------------------------------------------------------------------
  const scratchPath = mkdtempSync(join(tmpdir(), "provekit-overlay-"));

  // ------------------------------------------------------------------
  // Step 3: Add the git worktree (detached HEAD).
  //         On failure: remove scratch dir and rethrow.
  // ------------------------------------------------------------------
  try {
    execFileSync(
      "git",
      ["worktree", "add", "--detach", scratchPath, "HEAD"],
      { cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
    );
  } catch (err: unknown) {
    // Clean up the scratch dir we created before failing.
    rmSync(scratchPath, { recursive: true, force: true });
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`openOverlay: git worktree add failed: ${msg}`);
  }

  // ------------------------------------------------------------------
  // Step 4: Open and migrate a scratch SAST DB inside the worktree.
  // ------------------------------------------------------------------
  const sastDbDir = join(scratchPath, ".neurallog");
  mkdirSync(sastDbDir, { recursive: true });
  const sastDbPath = join(sastDbDir, "scratch-sast.db");
  const sastDb = openDb(sastDbPath);
  migrate(sastDb, { migrationsFolder: "./drizzle" });

  // ------------------------------------------------------------------
  // Step 5: Copy principles library (best-effort).
  // ------------------------------------------------------------------
  const principlesSrc = join(repoRoot, ".neurallog", "principles");
  const principlesDst = join(scratchPath, ".neurallog", "principles");
  if (existsSync(principlesSrc)) {
    try {
      cpSync(principlesSrc, principlesDst, { recursive: true });
    } catch {
      // Non-fatal — C3/C5 can cope without a principles dir.
    }
  }

  // ------------------------------------------------------------------
  // Step 6: Pre-index the locus file in the scratch DB.
  //
  // IMPORTANT: realpathSync both paths before computing relative().
  // On macOS, git rev-parse --show-toplevel resolves /var → /private/var
  // while mkdtempSync returns /var paths. Without normalization, relative()
  // produces a deep ../../ escape that resolves back to the original repo
  // file rather than the overlay copy.
  // ------------------------------------------------------------------
  const repoRootReal = realpathSync(repoRoot);
  const locusFileReal = realpathSync(locus.file);
  const relPath = relative(repoRootReal, locusFileReal);
  const scratchFilePath = join(scratchPath, relPath);
  if (existsSync(scratchFilePath)) {
    buildSASTForFile(sastDb, scratchFilePath);
  }

  // ------------------------------------------------------------------
  // Return the handle.
  // ------------------------------------------------------------------
  return {
    worktreePath: scratchPath,
    sastDbPath,
    sastDb,
    baseRef,
    modifiedFiles: new Set<string>(),
    closed: false,
  };
}
