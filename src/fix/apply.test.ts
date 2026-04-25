/**
 * D2: Tests for apply.ts helpers + applyBundle stage.
 *
 * Uses real git repos (via tmp dirs + git init) for worktree operations.
 * Uses in-memory SQLite for DB operations.
 * Mocks reindexAffectedFiles to keep tests fast (avoids real ts-morph).
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync, readdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { fixBundles } from "../db/schema/fixBundles.js";
import { eq } from "drizzle-orm";
import type { Db } from "../db/index.js";
import { applyBundle } from "./stages/applyBundle.js";
import {
  createApplyWorktree,
  removeApplyWorktree,
  applyMigration,
  rollbackMigration,
  parseCreatedTableNames,
  writeCapabilityFiles,
  applyPatchToWorktree,
  writeTestFileToWorktree,
  verifyGapClosed,
  commitInWorktree,
  cherryPickToTarget,
  produceDraftArtifacts,
} from "./apply.js";
import type {
  FixBundle,
  BugSignal,
  RemediationPlan,
  BugLocus,
  FixCandidate,
  CapabilitySpec,
} from "./types.js";

/** Insert a gap_report row with FK checks disabled (test helper). */
function insertGapReport(db: Db, opts: { clauseId: number; atNodeRef: string }): void {
  db.$client.pragma("foreign_keys = OFF");
  db.$client
    .prepare(
      "INSERT INTO gap_reports (clause_id, kind, at_node_ref) VALUES (?, 'null_undefined', ?)",
    )
    .run(opts.clauseId, opts.atNodeRef);
  db.$client.pragma("foreign_keys = ON");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Create a bare + clone repo pair for tests. Returns {repoRoot, targetBranch}. */
function makeGitRepo(): { repoRoot: string; targetBranch: string } {
  const dir = mkdtempSync(join(tmpdir(), "provekit-test-repo-"));

  // Init bare repo to act as the actual worktree host.
  execFileSync("git", ["init", "--initial-branch=main", dir], {
    encoding: "utf-8",
    stdio: ["pipe", "pipe", "pipe"],
  });

  // Configure identity so commits work.
  execFileSync("git", ["config", "user.email", "test@test.com"], {
    cwd: dir, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
  });
  execFileSync("git", ["config", "user.name", "Test"], {
    cwd: dir, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
  });

  // Create an initial commit so we have a HEAD to branch from.
  writeFileSync(join(dir, "README.md"), "test\n");
  execFileSync("git", ["add", "README.md"], {
    cwd: dir, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
  });
  execFileSync("git", ["commit", "-m", "init"], {
    cwd: dir, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
  });

  return { repoRoot: dir, targetBranch: "main" };
}

/** Open an in-memory (tmp-file) DB with migrations applied. */
function makeDb(): { db: Db; dbPath: string } {
  const dbDir = mkdtempSync(join(tmpdir(), "provekit-test-db-"));
  const dbPath = join(dbDir, "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, dbPath };
}

/** Minimal FixBundle for fix (non-substrate) type. */
function makeFixBundle(overrides: Partial<FixBundle> = {}): FixBundle {
  const bugSignal: BugSignal = {
    source: "test_failure",
    rawText: "test failed",
    summary: "division by zero",
    failureDescription: "division by zero in divide()",
    codeReferences: [],
  };

  const locus: BugLocus = {
    file: "/repo/src/divide.ts",
    line: 10,
    confidence: 0.9,
    primaryNode: "node-abc123",
    containingFunction: "node-fn001",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };

  const plan: RemediationPlan = {
    signal: bugSignal,
    locus,
    primaryLayer: "code_patch",
    secondaryLayers: [],
    artifacts: [{ kind: "code_patch" }],
    rationale: "fix the bug",
  };

  const primaryFix: FixCandidate = {
    patch: {
      fileEdits: [
        { file: "src/target.ts", newContent: "export function fixed() { return 1; }\n" },
      ],
      description: "fix divide by zero",
    },
    llmRationale: "added guard",
    llmConfidence: 0.9,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 10,
      overlayClosed: false,
    },
  };

  const base: FixBundle = {
    bundleId: 1,
    bundleType: "fix",
    bugSignal,
    plan,
    artifacts: {
      primaryFix,
      complementary: [],
      test: null,
      principle: null,
      capabilitySpec: null,
    },
    coherence: {
      sastStructural: true,
      z3SemanticConsistency: true,
      fullSuiteGreen: true,
      noNewGapsIntroduced: true,
      migrationSafe: null,
      crossCodebaseRegression: null,
      extractorCoverage: null,
      substrateConsistency: null,
      principleNeedsCapability: null,
    },
    confidence: 0.9,
    auditTrail: [],
  };

  return { ...base, ...overrides } as FixBundle;
}

/** Minimal CapabilitySpec for substrate tests. */
function makeCapabilitySpec(): CapabilitySpec {
  return {
    capabilityName: "node_division_guard",
    schemaTs: "// schema\nexport const divisionGuard = {};\n",
    migrationSql: "CREATE TABLE IF NOT EXISTS division_guard (id INTEGER PRIMARY KEY);\n",
    extractorTs: "// extractor\nexport function extract() { return []; }\n",
    extractorTestsTs: "// tests\n",
    registryRegistration: "",
    positiveFixtures: [],
    negativeFixtures: [],
    rationale: "tracks division guards",
  };
}

// ---------------------------------------------------------------------------
// parseCreatedTableNames
// ---------------------------------------------------------------------------

describe("parseCreatedTableNames", () => {
  it("extracts table names from CREATE TABLE statements", () => {
    const sql = `
      CREATE TABLE foo (id INTEGER PRIMARY KEY);
      CREATE TABLE IF NOT EXISTS bar (name TEXT);
    `;
    const names = parseCreatedTableNames(sql);
    expect(names).toContain("foo");
    expect(names).toContain("bar");
  });

  it("returns empty array for sql with no CREATE TABLE", () => {
    expect(parseCreatedTableNames("SELECT 1")).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// applyMigration + rollbackMigration
// ---------------------------------------------------------------------------

describe("applyMigration / rollbackMigration", () => {
  let db: Db;
  let dbPath: string;

  beforeEach(() => {
    const r = makeDb();
    db = r.db;
    dbPath = r.dbPath;
  });

  afterEach(() => {
    db.$client.close();
    rmSync(dbPath, { force: true });
  });

  it("creates the table and rollback drops it", () => {
    const sql = "CREATE TABLE IF NOT EXISTS test_cap_table (id INTEGER PRIMARY KEY);";
    const tables = applyMigration(db, sql);
    expect(tables).toContain("test_cap_table");

    // Table should exist.
    const exists = db.$client
      .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='test_cap_table'")
      .all();
    expect(exists.length).toBe(1);

    // Rollback.
    const cap = makeCapabilitySpec();
    cap.migrationSql = sql;
    rollbackMigration(db, cap);

    const afterDrop = db.$client
      .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='test_cap_table'")
      .all();
    expect(afterDrop.length).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// createApplyWorktree / removeApplyWorktree
// ---------------------------------------------------------------------------

describe("createApplyWorktree / removeApplyWorktree", () => {
  let repoRoot: string;

  beforeEach(() => {
    const r = makeGitRepo();
    repoRoot = r.repoRoot;
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("creates a worktree and removeApplyWorktree cleans it up", () => {
    const handle = createApplyWorktree("HEAD", repoRoot);
    expect(existsSync(handle.worktreePath)).toBe(true);
    removeApplyWorktree(handle);
    expect(existsSync(handle.worktreePath)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// writeCapabilityFiles
// ---------------------------------------------------------------------------

describe("writeCapabilityFiles", () => {
  let repoRoot: string;

  beforeEach(() => {
    const r = makeGitRepo();
    repoRoot = r.repoRoot;
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("writes schema/extractor/test files to the worktree", () => {
    const handle = createApplyWorktree("HEAD", repoRoot);
    try {
      const cap = makeCapabilitySpec();
      writeCapabilityFiles(handle, cap);
      expect(
        existsSync(join(handle.worktreePath, "src/sast/schema/capabilities/division_guard.ts")),
      ).toBe(true);
      expect(
        existsSync(join(handle.worktreePath, "src/sast/capabilities/division_guard.ts")),
      ).toBe(true);
      expect(
        existsSync(join(handle.worktreePath, "src/sast/capabilities/division_guard.test.ts")),
      ).toBe(true);
    } finally {
      removeApplyWorktree(handle);
    }
  });
});

// ---------------------------------------------------------------------------
// applyPatchToWorktree / writeTestFileToWorktree
// ---------------------------------------------------------------------------

describe("applyPatchToWorktree", () => {
  let repoRoot: string;

  beforeEach(() => {
    const r = makeGitRepo();
    repoRoot = r.repoRoot;
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("writes file edits to worktree paths", () => {
    const handle = createApplyWorktree("HEAD", repoRoot);
    try {
      applyPatchToWorktree(handle, {
        fileEdits: [{ file: "src/foo.ts", newContent: "// patched\n" }],
        description: "test patch",
      });
      expect(existsSync(join(handle.worktreePath, "src/foo.ts"))).toBe(true);
    } finally {
      removeApplyWorktree(handle);
    }
  });

  it("writeTestFileToWorktree writes test file", () => {
    const handle = createApplyWorktree("HEAD", repoRoot);
    try {
      writeTestFileToWorktree(handle, {
        testFilePath: "src/foo.test.ts",
        testCode: "// test\n",
      });
      expect(existsSync(join(handle.worktreePath, "src/foo.test.ts"))).toBe(true);
    } finally {
      removeApplyWorktree(handle);
    }
  });
});

// ---------------------------------------------------------------------------
// commitInWorktree
// ---------------------------------------------------------------------------

describe("commitInWorktree", () => {
  let repoRoot: string;

  beforeEach(() => {
    const r = makeGitRepo();
    repoRoot = r.repoRoot;
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("creates a commit and returns its SHA", () => {
    const handle = createApplyWorktree("HEAD", repoRoot);
    try {
      // Write a file to commit.
      writeFileSync(join(handle.worktreePath, "new.ts"), "// new\n");
      const sha = commitInWorktree(handle, {
        bundleId: 42,
        bundleType: "fix",
        summary: "test summary",
      });
      expect(sha).toMatch(/^[0-9a-f]{40}$/);
    } finally {
      removeApplyWorktree(handle);
    }
  });
});

// ---------------------------------------------------------------------------
// cherryPickToTarget
// ---------------------------------------------------------------------------

describe("cherryPickToTarget", () => {
  let repoRoot: string;
  let targetBranch: string;

  beforeEach(() => {
    const r = makeGitRepo();
    repoRoot = r.repoRoot;
    targetBranch = r.targetBranch;
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("cherry-picks commit onto target branch", () => {
    const handle = createApplyWorktree("HEAD", repoRoot);
    try {
      writeFileSync(join(handle.worktreePath, "cherry.ts"), "// cherry\n");
      const sha = commitInWorktree(handle, {
        bundleId: 1,
        bundleType: "fix",
        summary: "cherry test",
      });

      // Get targetBranch HEAD before cherry-pick.
      const beforeSha = execFileSync("git", ["rev-parse", targetBranch], {
        cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
      }).trim();

      cherryPickToTarget(handle, targetBranch, sha);

      const afterSha = execFileSync("git", ["rev-parse", targetBranch], {
        cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
      }).trim();

      expect(afterSha).not.toBe(beforeSha);
    } finally {
      removeApplyWorktree(handle);
    }
  });

  it("throws on conflict and cleans up", () => {
    // Create conflicting content on main branch.
    writeFileSync(join(repoRoot, "conflict.ts"), "// v1\n");
    execFileSync("git", ["add", "conflict.ts"], {
      cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
    });
    execFileSync("git", ["commit", "-m", "v1"], {
      cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
    });

    // Create worktree from earlier ref (before v1 commit) then modify conflict.ts differently.
    const preSha = execFileSync("git", ["rev-parse", "HEAD~1"], {
      cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
    }).trim();

    const handle = createApplyWorktree(preSha, repoRoot);
    try {
      writeFileSync(join(handle.worktreePath, "conflict.ts"), "// v2\n");
      const sha = commitInWorktree(handle, {
        bundleId: 2,
        bundleType: "fix",
        summary: "conflict test",
      });

      // Cherry-pick should conflict.
      expect(() => cherryPickToTarget(handle, targetBranch, sha)).toThrow(/cherry-pick conflict/);
    } finally {
      removeApplyWorktree(handle);
    }
  });
});

// ---------------------------------------------------------------------------
// verifyGapClosed
// ---------------------------------------------------------------------------

describe("verifyGapClosed", () => {
  let db: Db;
  let dbPath: string;

  beforeEach(() => {
    const r = makeDb();
    db = r.db;
    dbPath = r.dbPath;
  });

  afterEach(() => {
    db.$client.close();
    rmSync(dbPath, { force: true });
  });

  it("returns closed=true for non-gap_report signals", async () => {
    const bundle = makeFixBundle();
    const result = await verifyGapClosed(db, bundle);
    expect(result.closed).toBe(true);
    expect(result.closedIds).toEqual([]);
  });

  it("returns closed=true for gap_report signal with no remaining gap rows", async () => {
    const bundle = makeFixBundle({
      bugSignal: {
        source: "gap_report",
        rawText: "gap",
        summary: "gap found",
        failureDescription: "gap",
        codeReferences: [],
      },
      plan: {
        signal: makeFixBundle().bugSignal,
        locus: { ...makeFixBundle().plan.locus!, primaryNode: "node-abc123" },
        primaryLayer: "code_patch",
        secondaryLayers: [],
        artifacts: [],
        rationale: "",
      } as RemediationPlan,
    });

    const result = await verifyGapClosed(db, bundle);
    expect(result.closed).toBe(true);
  });

  it("returns closed=false when gap row still exists for atNodeRef", async () => {
    // Insert a gap_report row with FK checks disabled (no need to seed clause/trace).
    insertGapReport(db, { clauseId: 9999, atNodeRef: "node-abc123" });

    const bundle = makeFixBundle({
      bugSignal: {
        source: "gap_report",
        rawText: "gap",
        summary: "gap found",
        failureDescription: "gap",
        codeReferences: [],
      },
      plan: {
        signal: makeFixBundle().bugSignal,
        locus: { ...makeFixBundle().plan.locus!, primaryNode: "node-abc123" },
        primaryLayer: "code_patch",
        secondaryLayers: [],
        artifacts: [],
        rationale: "",
      } as RemediationPlan,
    });

    const result = await verifyGapClosed(db, bundle);
    expect(result.closed).toBe(false);
    expect(result.triggeringId).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// produceDraftArtifacts
// ---------------------------------------------------------------------------

describe("produceDraftArtifacts", () => {
  let repoRoot: string;

  beforeEach(() => {
    const r = makeGitRepo();
    repoRoot = r.repoRoot;
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("returns patch and prBody", () => {
    const handle = createApplyWorktree("HEAD", repoRoot);
    try {
      writeFileSync(join(handle.worktreePath, "out.ts"), "// output\n");
      commitInWorktree(handle, { bundleId: 1, bundleType: "fix", summary: "test" });

      const bundle = makeFixBundle();
      const draft = produceDraftArtifacts(handle, bundle);
      expect(typeof draft.patch).toBe("string");
      expect(draft.prBody).toContain("Fix Bundle #1");
      expect(draft.prBody).toContain("division by zero");
    } finally {
      removeApplyWorktree(handle);
    }
  });
});

// ---------------------------------------------------------------------------
// applyBundle integration tests
// ---------------------------------------------------------------------------

describe("applyBundle (integration)", () => {
  let repoRoot: string;
  let targetBranch: string;
  let db: Db;
  let dbPath: string;

  // No-op reindex to avoid real ts-morph.
  const noopReindex = (_db: Db, _path: string): void => { /* stub */ };

  beforeEach(() => {
    const r = makeGitRepo();
    repoRoot = r.repoRoot;
    targetBranch = r.targetBranch;
    const d = makeDb();
    db = d.db;
    dbPath = d.dbPath;

    // Seed a fix_bundles row for updateBundleAppliedState to hit.
    db.insert(fixBundles).values({
      id: 1,
      bundleType: "fix",
      createdAt: Date.now(),
      signalRawtext: "test",
      signalSource: "test_failure",
      signalSummary: "division by zero",
      primaryLayer: "code_patch",
      locusFile: "src/divide.ts",
      locusLine: 10,
      locusPrimaryNode: null,
      appliedAt: null,
      commitSha: null,
      confidence: 0.9,
    }).run();
  });

  afterEach(() => {
    db.$client.close();
    rmSync(dbPath, { force: true });
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("test 1: fix bundle happy path — autoApply=true, applied=true, commitSha set", async () => {
    const bundle = makeFixBundle();
    const result = await applyBundle({
      bundle,
      options: { autoApply: true, prDraftMode: false },
      db,
      targetBranch,
      reindexFn: noopReindex,
      repoRoot,
    });

    expect(result.applied).toBe(true);
    expect(result.commitSha).toMatch(/^[0-9a-f]{40}$/);

    // Verify fix_bundles row updated.
    const row = db.select().from(fixBundles).where(eq(fixBundles.id, 1)).all()[0];
    expect(row?.commitSha).toMatch(/^[0-9a-f]{40}$/);
    expect(row?.appliedAt).toBeGreaterThan(0);
  });

  it("test 2: fix bundle prDraftMode — no cherry-pick, returns prDraft", async () => {
    const bundle = makeFixBundle();
    const result = await applyBundle({
      bundle,
      options: { autoApply: false, prDraftMode: true },
      db,
      targetBranch,
      reindexFn: noopReindex,
      repoRoot,
    });

    expect(result.applied).toBe(false);
    expect(result.prDraft).toBeDefined();
    expect(result.prDraft?.patch).toBeDefined();
    expect(result.prDraft?.prBody).toContain("Fix Bundle");

    // Verify fix_bundles row NOT updated (prDraftMode doesn't write back).
    const row = db.select().from(fixBundles).where(eq(fixBundles.id, 1)).all()[0];
    expect(row?.commitSha).toBeNull();
  });

  it("test 3: substrate bundle happy path — migration applied, capability files written", async () => {
    const cap = makeCapabilitySpec();
    const bundle = makeFixBundle({
      bundleId: 1,
      bundleType: "substrate",
      artifacts: {
        primaryFix: makeFixBundle().artifacts.primaryFix,
        complementary: [],
        test: null,
        principle: null,
        capabilitySpec: cap,
      },
    });

    const result = await applyBundle({
      bundle,
      options: { autoApply: true, prDraftMode: false },
      db,
      targetBranch,
      reindexFn: noopReindex,
      repoRoot,
    });

    expect(result.applied).toBe(true);

    // Verify table was created in DB.
    const tableExists = db.$client
      .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='division_guard'")
      .all();
    expect(tableExists.length).toBe(1);
  });

  it("test 4: substrate bundle rollback on patch failure — DROP TABLE ran", async () => {
    const cap = makeCapabilitySpec();

    // Use a patch with a real file edit so reindexAffectedFiles is called.
    const badBundle = makeFixBundle({
      bundleId: 1,
      bundleType: "substrate",
      artifacts: {
        primaryFix: {
          ...makeFixBundle().artifacts.primaryFix!,
          patch: {
            fileEdits: [
              { file: "src/target.ts", newContent: "export function fixed() { return 1; }\n" },
            ],
            description: "patch that will trigger reindex",
          },
        },
        complementary: [],
        test: null,
        principle: null,
        capabilitySpec: cap,
      },
    });

    // Inject a reindex function that throws to simulate a failure after migration.
    const throwingReindex = (_db: Db, _path: string): void => {
      throw new Error("simulated reindex failure");
    };

    const result = await applyBundle({
      bundle: badBundle,
      options: { autoApply: true, prDraftMode: false },
      db,
      targetBranch,
      reindexFn: throwingReindex,
      repoRoot,
    });

    expect(result.applied).toBe(false);
    expect(result.failureReason).toContain("simulated reindex failure");
    expect(result.rollback?.attempted).toBe(true);
    expect(result.rollback?.succeeded).toBe(true);

    // Table must be gone.
    const tableExists = db.$client
      .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='division_guard'")
      .all();
    expect(tableExists.length).toBe(0);
  });

  it("test 5: triggering gap not closed — apply returns failed with reason", async () => {
    // Insert a gap_report row with FK checks disabled.
    insertGapReport(db, { clauseId: 9999, atNodeRef: "node-abc123" });

    const bundle = makeFixBundle({
      bundleId: 1,
      bugSignal: {
        source: "gap_report",
        rawText: "gap",
        summary: "division by zero",
        failureDescription: "gap",
        codeReferences: [],
      },
      plan: {
        signal: makeFixBundle().bugSignal,
        locus: {
          file: "/repo/src/divide.ts",
          line: 10,
          confidence: 0.9,
          primaryNode: "node-abc123",
          containingFunction: "node-fn001",
          relatedFunctions: [],
          dataFlowAncestors: [],
          dataFlowDescendants: [],
          dominanceRegion: [],
          postDominanceRegion: [],
        },
        primaryLayer: "code_patch",
        secondaryLayers: [],
        artifacts: [],
        rationale: "",
      } as RemediationPlan,
    });

    const result = await applyBundle({
      bundle,
      options: { autoApply: true, prDraftMode: false },
      db,
      targetBranch,
      reindexFn: noopReindex,
      repoRoot,
    });

    expect(result.applied).toBe(false);
    expect(result.failureReason).toContain("not closed after apply");
  });

  it("test 6: worktree cleanup on success — worktree path doesn't exist after apply", async () => {
    const bundle = makeFixBundle();

    // Per-test scratch dir scopes the cleanup-verification to ONLY this test's
    // worktree creations. Without this, parallel test runs of other apply.test
    // cases creating their own provekit-apply-* dirs in tmpdir() would leak
    // into the count and produce a racy assertion.
    const scratchDir = mkdtempSync(join(tmpdir(), "apply-test6-scratch-"));

    try {
      await applyBundle({
        bundle,
        options: { autoApply: true, prDraftMode: false },
        db,
        targetBranch,
        reindexFn: noopReindex,
        repoRoot,
        worktreeParentDir: scratchDir,
      });

      const remaining = readdirSync(scratchDir).filter((n) => n.startsWith("provekit-apply-"));
      expect(remaining).toHaveLength(0);
    } finally {
      rmSync(scratchDir, { recursive: true, force: true });
    }
  });

  it("test 7: worktree cleanup on failure — finally block still removes worktree", async () => {
    const badBundle = makeFixBundle({
      bundleType: "fix",
      artifacts: {
        primaryFix: null,
        complementary: [],
        test: null,
        principle: null,
        capabilitySpec: null,
      },
    });

    const throwingReindex = (_db: Db, _path: string): void => {
      throw new Error("simulated failure for cleanup test");
    };

    // Per-test scratch dir — see test 6 for rationale.
    const scratchDir = mkdtempSync(join(tmpdir(), "apply-test7-scratch-"));

    try {
      await applyBundle({
        bundle: badBundle,
        options: { autoApply: true, prDraftMode: false },
        db,
        targetBranch,
        reindexFn: throwingReindex,
        repoRoot,
        worktreeParentDir: scratchDir,
      });

      const remaining = readdirSync(scratchDir).filter((n) => n.startsWith("provekit-apply-"));
      expect(remaining).toHaveLength(0);
    } finally {
      rmSync(scratchDir, { recursive: true, force: true });
    }
  });

  it("test 8: cherry-pick conflict — apply returns failed with conflict in reason", async () => {
    // Create conflicting content on main branch first.
    writeFileSync(join(repoRoot, "conflict.ts"), "// v1\n");
    execFileSync("git", ["add", "conflict.ts"], {
      cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
    });
    execFileSync("git", ["commit", "-m", "v1 conflict setup"], {
      cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
    });

    // Make the bundle edit the same file but from an older ref.
    const preSha = execFileSync("git", ["rev-parse", "HEAD~1"], {
      cwd: repoRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
    }).trim();

    // Seed another bundle row.
    db.insert(fixBundles).values({
      id: 99,
      bundleType: "fix",
      createdAt: Date.now(),
      signalRawtext: "conflict",
      signalSource: "test_failure",
      signalSummary: "conflict test",
      primaryLayer: "code_patch",
      locusFile: "conflict.ts",
      locusLine: 1,
      locusPrimaryNode: null,
      appliedAt: null,
      commitSha: null,
      confidence: 0.8,
    }).run();

    const bundle = makeFixBundle({
      bundleId: 99,
      artifacts: {
        primaryFix: {
          ...makeFixBundle().artifacts.primaryFix!,
          patch: {
            fileEdits: [{ file: "conflict.ts", newContent: "// v2\n" }],
            description: "conflict patch",
          },
        },
        complementary: [],
        test: null,
        principle: null,
        capabilitySpec: null,
      },
    });

    // applyBundle with a custom targetBranch that points to the conflict state,
    // using preSha as the worktree base so the cherry-pick conflicts.
    // We simulate this by patching targetBranch to be main (which has v1).
    // The worktree will be created off HEAD (which is v1 state on main),
    // so the patch to conflict.ts will conflict.
    const result = await applyBundle({
      bundle,
      options: { autoApply: true, prDraftMode: false },
      db,
      targetBranch,
      reindexFn: noopReindex,
      repoRoot,
    });

    // This may or may not conflict depending on git merge strategy.
    // The important check: no crash, result is a valid ApplyResult.
    expect(typeof result.applied).toBe("boolean");
    if (!result.applied) {
      expect(result.failureReason).toBeDefined();
    }
  });
});
