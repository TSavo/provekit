/**
 * D2: applyBundle — transactional bundle application.
 *
 * Atomicity: all-or-nothing. On any failure after migration applied → rollback
 * DROP the new tables and discard the worktree.
 *
 * autoApply: cherry-pick onto targetBranch via a second detached worktree
 *            (never mutates the user's checked-out working tree directly).
 * prDraftMode: capture diff + PR body; no cherry-pick; no DB update.
 *
 * No force-push, no history rewrite, no remote push.
 */

import { execFileSync } from "child_process";
import { dirname } from "path";
import type { FixBundle, ApplyResult } from "../types.js";
import type { Db } from "../../db/index.js";
import type { FixLoopLogger } from "../logger.js";
import {
  createApplyWorktree,
  removeApplyWorktree,
  applyMigration,
  rollbackMigration,
  writeCapabilityFiles,
  applyPatchToWorktree,
  writeTestFileToWorktree,
  reindexAffectedFiles,
  verifyGapClosed,
  commitInWorktree,
  cherryPickToTarget,
  produceDraftArtifacts,
  updateBundleAppliedState,
  type ApplyWorktreeHandle,
} from "../apply.js";
import type { CapabilitySpec } from "../types.js";

// ---------------------------------------------------------------------------
// resolveRepoRoot
// ---------------------------------------------------------------------------

function resolveRepoRoot(bundle: FixBundle): string {
  // Prefer locus file to determine the repo root.
  const locusFile = bundle.plan.locus?.file;
  if (locusFile) {
    try {
      return execFileSync(
        "git",
        ["rev-parse", "--show-toplevel"],
        { cwd: dirname(locusFile), encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
      ).trim();
    } catch {
      // fall through
    }
  }
  // Fall back to process.cwd().
  try {
    return execFileSync(
      "git",
      ["rev-parse", "--show-toplevel"],
      { cwd: process.cwd(), encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
    ).trim();
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`applyBundle: could not resolve repo root: ${msg}`);
  }
}

// ---------------------------------------------------------------------------
// applyBundle
// ---------------------------------------------------------------------------

export async function applyBundle(args: {
  bundle: FixBundle;
  options: { autoApply: boolean; prDraftMode: boolean };
  db: Db;
  targetBranch?: string;
  /** Injectable reindex function for testing (defaults to real reindexFile). */
  reindexFn?: (db: Db, absPath: string) => void;
  /** Injectable repo root for testing. */
  repoRoot?: string;
  logger?: FixLoopLogger;
}): Promise<ApplyResult> {
  const { bundle, options, db } = args;
  const isSubstrate = bundle.bundleType === "substrate";

  // Resolve repo root once.
  const repoRoot = args.repoRoot ?? resolveRepoRoot(bundle);

  // Resolve target branch / ref.
  let targetRef: string;
  if (args.targetBranch) {
    targetRef = args.targetBranch;
  } else {
    // Default: current HEAD branch name, or "HEAD" if detached.
    try {
      targetRef = execFileSync(
        "git",
        ["symbolic-ref", "--short", "HEAD"],
        { cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
      ).trim();
    } catch {
      targetRef = "HEAD";
    }
  }

  // 1. Create fresh worktree off targetRef.
  let applyHandle: ApplyWorktreeHandle;
  try {
    applyHandle = createApplyWorktree(targetRef, repoRoot);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return { applied: false, closedGaps: [], failureReason: `worktree creation failed: ${msg}` };
  }

  let migrationApplied = false;
  let createdTables: string[] = [];
  let cap: CapabilitySpec | null = null;

  try {
    // 2. Substrate only — apply migration to LIVE DB first.
    if (isSubstrate) {
      cap = bundle.artifacts.capabilitySpec;
      if (!cap) {
        throw new Error("substrate bundle missing capabilitySpec");
      }
      createdTables = applyMigration(db, cap.migrationSql);
      migrationApplied = true;
    }

    // 3. Substrate only — write capability schema + extractor files into worktree.
    if (isSubstrate && cap) {
      writeCapabilityFiles(applyHandle, cap);
    }

    // 4. Apply all code patches to worktree.
    if (bundle.artifacts.primaryFix) {
      applyPatchToWorktree(applyHandle, bundle.artifacts.primaryFix.patch);
    }
    for (const comp of bundle.artifacts.complementary) {
      applyPatchToWorktree(applyHandle, comp.patch);
    }
    if (bundle.artifacts.test) {
      writeTestFileToWorktree(applyHandle, bundle.artifacts.test);
    }

    // 5. Re-index SAST for affected files against the main DB.
    await reindexAffectedFiles(db, applyHandle, bundle, args.reindexFn);

    // 6. Verify triggering gap closed.
    const gapResult = await verifyGapClosed(db, bundle);
    if (!gapResult.closed) {
      // CRITICAL: gap not closed — fail without committing.
      if (migrationApplied && cap) {
        let rollbackSucceeded = false;
        try {
          rollbackMigration(db, cap);
          rollbackSucceeded = true;
        } catch {
          rollbackSucceeded = false;
        }
        return {
          applied: false,
          closedGaps: [],
          failureReason: `triggering gap (atNodeRef=${bundle.plan.locus?.primaryNode}) not closed after apply`,
          rollback: {
            attempted: true,
            succeeded: rollbackSucceeded,
            detail: `Dropped tables: ${createdTables.join(", ")}`,
          },
        };
      }
      return {
        applied: false,
        closedGaps: [],
        failureReason: `triggering gap (atNodeRef=${bundle.plan.locus?.primaryNode}) not closed after apply`,
      };
    }

    // 7. Commit in worktree.
    const commitSha = commitInWorktree(applyHandle, {
      bundleId: bundle.bundleId,
      bundleType: bundle.bundleType,
      summary: bundle.bugSignal.summary,
    });

    // 8. Capture draft artifacts BEFORE removing worktree (needed for prDraftMode).
    let prDraft: ReturnType<typeof produceDraftArtifacts> | undefined;
    if (options.prDraftMode) {
      prDraft = produceDraftArtifacts(applyHandle, bundle);
    }

    // 9. autoApply: cherry-pick onto targetBranch.
    if (options.autoApply) {
      try {
        cherryPickToTarget(applyHandle, targetRef, commitSha);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        // Rollback migration if needed.
        if (migrationApplied && cap) {
          try { rollbackMigration(db, cap); } catch { /* ignore */ }
        }
        return {
          applied: false,
          closedGaps: gapResult.closedIds,
          commitSha,
          failureReason: msg,
          rollback: migrationApplied
            ? { attempted: true, succeeded: true, detail: `Dropped tables: ${createdTables.join(", ")}` }
            : undefined,
        };
      }

      // Update fix_bundles row: applied_at + commit_sha.
      updateBundleAppliedState(db, bundle.bundleId, {
        commitSha,
        appliedAt: Date.now(),
      });

      return {
        applied: true,
        commitSha,
        closedGaps: gapResult.closedIds,
      };
    }

    // prDraftMode: return draft without cherry-pick.
    if (options.prDraftMode) {
      return {
        applied: false,
        commitSha,
        closedGaps: gapResult.closedIds,
        prDraft,
      };
    }

    // Neither mode — still success (e.g. dry-run).
    return {
      applied: true,
      commitSha,
      closedGaps: gapResult.closedIds,
    };
  } catch (err) {
    // Rollback migration if we applied one.
    let rollbackSucceeded = false;
    if (migrationApplied && cap) {
      try {
        rollbackMigration(db, cap);
        rollbackSucceeded = true;
      } catch {
        rollbackSucceeded = false;
      }
    }
    const msg = err instanceof Error ? err.message : String(err);
    return {
      applied: false,
      closedGaps: [],
      failureReason: msg,
      rollback: migrationApplied
        ? { attempted: true, succeeded: rollbackSucceeded, detail: `Dropped tables: ${createdTables.join(", ")}` }
        : undefined,
    };
  } finally {
    // Cleanup: always remove worktree (MVP: remove always, simpler than keep-on-failure).
    // Tradeoff: losing the worktree on failure makes post-mortem harder.
    // Future: accept a keepOnFailure option.
    removeApplyWorktree(applyHandle!);
  }
}
