/**
 * D2: Apply helpers — transactional bundle application.
 *
 * All git shell-outs use execFileSync for simplicity.
 * No force-push, no history rewrite, no remote push.
 */

import { writeFileSync, mkdirSync, existsSync, rmSync } from "fs";
import { join, dirname } from "path";
import { mkdtempSync } from "fs";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { eq } from "drizzle-orm";
import { fixBundles } from "../db/schema/fixBundles.js";
import { gapReports } from "../db/schema/gapReports.js";
import { reindexFile } from "../sast/builder.js";
import type { Db } from "../db/index.js";
import type {
  CapabilitySpec,
  FixBundle,
  CodePatch,
  PrDraftArtifacts,
} from "./types.js";

// ---------------------------------------------------------------------------
// ApplyWorktreeHandle
// ---------------------------------------------------------------------------

export interface ApplyWorktreeHandle {
  /** Absolute path to the worktree on disk. */
  worktreePath: string;
  /** The git ref the worktree was created from (branch name or SHA). */
  baseRef: string;
  /** Absolute path to the original repository root (not the worktree root). */
  repoRoot: string;
}

// ---------------------------------------------------------------------------
// createApplyWorktree
// ---------------------------------------------------------------------------

/**
 * Create a fresh git worktree off `ref` in a tmp directory.
 *
 * The caller must call removeApplyWorktree() when done.
 * We need a repo root to register the worktree — we derive it from cwd.
 */
export function createApplyWorktree(
  ref: string,
  repoRoot: string,
  parentDir?: string,
): ApplyWorktreeHandle {
  // parentDir lets tests scope worktrees to a per-test scratch directory so
  // their cleanup-verification assertions aren't poisoned by concurrent runs
  // of other apply.test cases that also create provekit-apply-* dirs in
  // tmpdir(). Production callers omit it and get the default tmpdir().
  const worktreePath = mkdtempSync(join(parentDir ?? tmpdir(), "provekit-apply-"));

  try {
    execFileSync("git", ["worktree", "add", "--detach", worktreePath, ref], {
      cwd: repoRoot,
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
  } catch (err) {
    rmSync(worktreePath, { recursive: true, force: true });
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`createApplyWorktree: git worktree add failed: ${msg}`);
  }

  return { worktreePath, baseRef: ref, repoRoot };
}

// ---------------------------------------------------------------------------
// removeApplyWorktree
// ---------------------------------------------------------------------------

export function removeApplyWorktree(handle: ApplyWorktreeHandle): void {
  // Try the git command first — it cleans both the filesystem directory
  // AND the .git/worktrees/ admin entry. We ignore its failure (captured
  // in gitErr only for diagnostics if everything else also fails).
  let gitErr: unknown = null;
  try {
    execFileSync("git", ["worktree", "remove", "--force", handle.worktreePath], {
      cwd: handle.repoRoot,
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
  } catch (e) {
    gitErr = e;
  }

  // REGARDLESS of git's outcome, make sure the filesystem directory is gone.
  // Under parallel-test load, git's admin lock can race and the --force command
  // can silently half-succeed (admin removed, FS kept or vice versa). The
  // filesystem cleanup here is the definitive step.
  if (existsSync(handle.worktreePath)) {
    try {
      rmSync(handle.worktreePath, { recursive: true, force: true });
    } catch (e) {
      // Genuinely broken — surface both failure modes together.
      throw new Error(
        `removeApplyWorktree failed: rmSync on ${handle.worktreePath} → ${e instanceof Error ? e.message : String(e)}` +
          (gitErr ? ` (git worktree remove also failed: ${gitErr instanceof Error ? gitErr.message : String(gitErr)})` : ""),
      );
    }
  }

  // Prune any stale admin entry the git command may have left behind.
  // Safe to run even when nothing's stale; errors here are non-fatal.
  try {
    execFileSync("git", ["worktree", "prune"], {
      cwd: handle.repoRoot,
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
  } catch {
    // Pruning is hygiene; not worth blocking cleanup on.
  }
}

// ---------------------------------------------------------------------------
// applyMigration
// ---------------------------------------------------------------------------

/**
 * Parse table names from CREATE TABLE statements in a SQL string.
 * Returns each matched table name.
 */
export function parseCreatedTableNames(sql: string): string[] {
  const names: string[] = [];
  const re = /CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"']?(\w+)[`"']?/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(sql)) !== null) {
    names.push(m[1]);
  }
  return names;
}

/**
 * Apply a migration SQL string to the live DB.
 * Uses better-sqlite3's exec() which handles multi-statement SQL natively.
 * Wrapped in a transaction so the entire migration is atomic.
 *
 * Returns the list of table names that were created (for rollback).
 */
export function applyMigration(db: Db, sql: string): string[] {
  const tableNames = parseCreatedTableNames(sql);

  db.transaction(() => {
    db.$client.exec(sql);
  });

  return tableNames;
}

// ---------------------------------------------------------------------------
// rollbackMigration
// ---------------------------------------------------------------------------

/**
 * Drop tables that were created by applyMigration.
 * Safe: tables are new (no pre-existing data). Indexes drop automatically.
 *
 * Constraint: substrate migrations are CREATE TABLE only — no ALTERs, no seeds.
 */
export function rollbackMigration(db: Db, cap: CapabilitySpec): void {
  const tableNames = parseCreatedTableNames(cap.migrationSql);

  db.transaction(() => {
    for (const name of tableNames) {
      db.$client.exec(`DROP TABLE IF EXISTS ${JSON.stringify(name)}`);
    }
  });
}

// ---------------------------------------------------------------------------
// writeCapabilityFiles
// ---------------------------------------------------------------------------

/**
 * Write capability schema/extractor/test files into the apply worktree.
 *
 * Path derivation:
 *   schema   → src/sast/schema/capabilities/<slug>.ts
 *   extractor → src/sast/capabilities/<slug>.ts
 *   tests    → src/sast/capabilities/<slug>.test.ts
 */
export function writeCapabilityFiles(
  handle: ApplyWorktreeHandle,
  cap: CapabilitySpec,
): void {
  const slug = cap.capabilityName.replace(/^node_/, "");

  const paths: Array<{ content: string; rel: string }> = [
    {
      content: cap.schemaTs,
      rel: `src/sast/schema/capabilities/${slug}.ts`,
    },
    {
      content: cap.extractorTs,
      rel: `src/sast/capabilities/${slug}.ts`,
    },
    {
      content: cap.extractorTestsTs,
      rel: `src/sast/capabilities/${slug}.test.ts`,
    },
  ];

  for (const { content, rel } of paths) {
    const absPath = join(handle.worktreePath, rel);
    mkdirSync(dirname(absPath), { recursive: true });
    writeFileSync(absPath, content, "utf8");
  }
}

// ---------------------------------------------------------------------------
// applyPatchToWorktree
// ---------------------------------------------------------------------------

export function applyPatchToWorktree(
  handle: ApplyWorktreeHandle,
  patch: CodePatch,
): void {
  for (const edit of patch.fileEdits) {
    const absPath = join(handle.worktreePath, edit.file);
    mkdirSync(dirname(absPath), { recursive: true });
    writeFileSync(absPath, edit.newContent, "utf8");
  }
}

// ---------------------------------------------------------------------------
// writeTestFileToWorktree
// ---------------------------------------------------------------------------

export function writeTestFileToWorktree(
  handle: ApplyWorktreeHandle,
  test: { testFilePath: string; testCode: string },
): void {
  const absPath = join(handle.worktreePath, test.testFilePath);
  mkdirSync(dirname(absPath), { recursive: true });
  writeFileSync(absPath, test.testCode, "utf8");
}

// ---------------------------------------------------------------------------
// reindexAffectedFiles
// ---------------------------------------------------------------------------

/**
 * Re-index files touched by this bundle against the MAIN DB.
 * Represents the new state of the codebase post-apply.
 *
 * injectable: pass a custom reindex function for tests (avoids real ts-morph).
 */
export async function reindexAffectedFiles(
  db: Db,
  handle: ApplyWorktreeHandle,
  bundle: FixBundle,
  reindexFn: (db: Db, absPath: string) => void = reindexFile,
): Promise<void> {
  const files = new Set<string>();

  if (bundle.artifacts.primaryFix) {
    for (const edit of bundle.artifacts.primaryFix.patch.fileEdits) {
      files.add(edit.file);
    }
  }
  for (const comp of bundle.artifacts.complementary) {
    for (const edit of comp.patch.fileEdits) {
      files.add(edit.file);
    }
  }
  if (bundle.artifacts.test) {
    files.add(bundle.artifacts.test.testFilePath);
  }

  for (const rel of files) {
    const absPath = join(handle.worktreePath, rel);
    if (existsSync(absPath)) {
      reindexFn(db, absPath);
    }
  }
}

// ---------------------------------------------------------------------------
// verifyGapClosed
// ---------------------------------------------------------------------------

/**
 * For gap_report-sourced signals: verify the triggering gap_reports row
 * is gone from the DB after reindex.
 *
 * Since BugSignal doesn't carry a gapId, we match by atNodeRef === locus.primaryNode.
 * If no primaryNode is set, we skip verification (return closed=true).
 *
 * For non-gap-report signals: always returns closed=true.
 */
export async function verifyGapClosed(
  db: Db,
  bundle: FixBundle,
): Promise<{ closed: boolean; closedIds: number[]; triggeringId?: number }> {
  if (bundle.bugSignal.source !== "gap_report") {
    return { closed: true, closedIds: [] };
  }

  const primaryNode = bundle.plan.locus?.primaryNode;
  if (!primaryNode) {
    // Can't verify without a node ref — treat as closed.
    return { closed: true, closedIds: [] };
  }

  const remaining = db
    .select()
    .from(gapReports)
    .where(eq(gapReports.atNodeRef, primaryNode))
    .all();

  if (remaining.length === 0) {
    return { closed: true, closedIds: [] };
  }

  // Still present — the fix didn't close the gap.
  return {
    closed: false,
    closedIds: [],
    triggeringId: remaining[0]?.id,
  };
}

// ---------------------------------------------------------------------------
// commitInWorktree
// ---------------------------------------------------------------------------

export function commitInWorktree(
  handle: ApplyWorktreeHandle,
  meta: { bundleId: number; bundleType: string; summary: string },
): string {
  const message = [
    `fix: bundle #${meta.bundleId} (${meta.bundleType}) — ${meta.summary}`,
    "",
    "Auto-generated by provekit fix loop.",
  ].join("\n");

  execFileSync("git", ["add", "-A"], {
    cwd: handle.worktreePath,
    encoding: "utf-8",
    stdio: ["pipe", "pipe", "pipe"],
  });

  execFileSync("git", ["commit", "-m", message, "--allow-empty"], {
    cwd: handle.worktreePath,
    encoding: "utf-8",
    stdio: ["pipe", "pipe", "pipe"],
  });

  const sha = execFileSync("git", ["rev-parse", "HEAD"], {
    cwd: handle.worktreePath,
    encoding: "utf-8",
    stdio: ["pipe", "pipe", "pipe"],
  }).trim();

  return sha;
}

// ---------------------------------------------------------------------------
// cherryPickToTarget
// ---------------------------------------------------------------------------

/**
 * Cherry-pick `sha` onto `targetBranch` in the original repo.
 *
 * We create a SECOND detached worktree off targetBranch to avoid touching
 * the user's checked-out working tree. After the cherry-pick we update the
 * branch ref to point at the new commit, then remove the helper worktree.
 *
 * On conflict: abort cherry-pick, remove helper worktree, throw.
 */
export function cherryPickToTarget(
  handle: ApplyWorktreeHandle,
  targetBranch: string,
  sha: string,
): void {
  const helperPath = mkdtempSync(join(tmpdir(), "provekit-cherry-"));

  try {
    // Create a detached worktree off targetBranch.
    execFileSync(
      "git",
      ["worktree", "add", "--detach", helperPath, targetBranch],
      {
        cwd: handle.repoRoot,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      },
    );

    // Cherry-pick the commit.
    try {
      execFileSync("git", ["cherry-pick", sha], {
        cwd: helperPath,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      });
    } catch (err) {
      // Abort on conflict.
      try {
        execFileSync("git", ["cherry-pick", "--abort"], {
          cwd: helperPath,
          encoding: "utf-8",
          stdio: ["pipe", "pipe", "pipe"],
        });
      } catch {
        // ignore abort errors
      }
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(`cherryPickToTarget: cherry-pick conflict: ${msg}`);
    }

    // Get the new HEAD sha in the helper worktree.
    const newSha = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: helperPath,
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();

    // Update the branch ref to point at the cherry-picked commit.
    // Use update-ref instead of branch -f to allow updating the checked-out branch.
    execFileSync("git", ["update-ref", `refs/heads/${targetBranch}`, newSha], {
      cwd: handle.repoRoot,
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
  } finally {
    // Remove the helper worktree in all cases.
    try {
      execFileSync(
        "git",
        ["worktree", "remove", "--force", helperPath],
        {
          cwd: handle.repoRoot,
          encoding: "utf-8",
          stdio: ["pipe", "pipe", "pipe"],
        },
      );
    } catch {
      if (existsSync(helperPath)) {
        rmSync(helperPath, { recursive: true, force: true });
      }
    }
  }
}

// ---------------------------------------------------------------------------
// produceDraftArtifacts
// ---------------------------------------------------------------------------

export function produceDraftArtifacts(
  handle: ApplyWorktreeHandle,
  bundle: FixBundle,
): PrDraftArtifacts {
  // Capture the diff of this commit vs the base ref.
  let patch: string;
  try {
    patch = execFileSync(
      "git",
      ["diff", `${handle.baseRef}..HEAD`],
      {
        cwd: handle.worktreePath,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      },
    );
  } catch {
    patch = "(diff unavailable)";
  }

  const lines: string[] = [
    `## Fix Bundle #${bundle.bundleId} (${bundle.bundleType})`,
    "",
    `**Signal:** ${bundle.bugSignal.summary}`,
    "",
    `**Confidence:** ${(bundle.confidence * 100).toFixed(1)}%`,
    "",
    "### Changes",
  ];

  if (bundle.artifacts.primaryFix) {
    lines.push(
      `- **Primary fix:** ${bundle.artifacts.primaryFix.patch.description}`,
    );
  }
  for (const comp of bundle.artifacts.complementary) {
    lines.push(`- **Complementary (${comp.kind}):** ${comp.rationale}`);
  }
  if (bundle.artifacts.test) {
    lines.push(
      `- **Regression test:** \`${bundle.artifacts.test.testFilePath}\``,
    );
  }
  if (bundle.artifacts.capabilitySpec) {
    lines.push(
      `- **Capability:** \`${bundle.artifacts.capabilitySpec.capabilityName}\``,
    );
  }

  lines.push("", "### Oracle Coherence");
  const coh = bundle.coherence;
  lines.push(`- SAST structural: ${coh.sastStructural}`);
  lines.push(`- Z3 semantic consistency: ${coh.z3SemanticConsistency}`);
  lines.push(`- Full suite green: ${coh.fullSuiteGreen}`);
  lines.push(`- No new gaps: ${coh.noNewGapsIntroduced}`);

  lines.push("", "_Auto-generated by provekit fix loop._");

  return { patch, prBody: lines.join("\n") };
}

// ---------------------------------------------------------------------------
// updateBundleAppliedState
// ---------------------------------------------------------------------------

export function updateBundleAppliedState(
  db: Db,
  bundleId: number,
  state: { commitSha: string; appliedAt: number },
): void {
  db.update(fixBundles)
    .set({ commitSha: state.commitSha, appliedAt: state.appliedAt })
    .where(eq(fixBundles.id, bundleId))
    .run();
}
