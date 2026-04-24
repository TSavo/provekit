/**
 * C2: Overlay tests — scratch worktree + scratch SAST DB lifecycle.
 *
 * Each test creates its own tempdir + git repo so they have no shared state.
 */

import { describe, it, expect, afterEach } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  readFileSync,
  existsSync,
} from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { files } from "../sast/schema/index.js";
import { nodes } from "../sast/schema/index.js";
import { eq } from "drizzle-orm";
import { openOverlay } from "./stages/openOverlay.js";
import { applyPatchToOverlay, reindexOverlay, closeOverlay } from "./overlay.js";
import type { BugLocus } from "./types.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Git config flags for test commits — avoids "please tell me who you are" errors. */
const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];

/** Create a fresh git repo with one committed TypeScript fixture. */
function makeTestRepo(): { repoDir: string; fixturePath: string } {
  const repoDir = mkdtempSync(join(tmpdir(), "provekit-overlay-test-repo-"));
  execFileSync("git", [...GIT_ID, "init", repoDir]);
  execFileSync("git", [...GIT_ID, "init"], { cwd: repoDir });

  const fixturePath = join(repoDir, "fixture.ts");
  writeFileSync(fixturePath, "export function add(a: number, b: number) { return a + b; }", "utf8");

  execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
  execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

  return { repoDir, fixturePath };
}

/** Open a main DB against a tempdir (separate from any overlay). */
function openMainDb(tmpDir: string) {
  const dbPath = join(tmpDir, "main.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, dbPath };
}

/** Minimal BugLocus pointing at a file. */
function makeLocus(filePath: string): BugLocus {
  const nodeId = "aaaa000000000000";
  return {
    file: filePath,
    line: 1,
    confidence: 1.0,
    primaryNode: nodeId,
    containingFunction: nodeId,
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("C2 overlay lifecycle", () => {
  // Track resources for afterEach cleanup in case a test crashes mid-way.
  const cleanups: (() => void)[] = [];

  afterEach(() => {
    for (const fn of cleanups.splice(0)) {
      try { fn(); } catch { /* ignore */ }
    }
  });

  // -------------------------------------------------------------------------
  // 1. openOverlay creates scratch worktree + scratch DB; closeOverlay prunes
  // -------------------------------------------------------------------------
  it("openOverlay creates scratch worktree, scratch DB, and prunes both on closeOverlay", async () => {
    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-overlay-test-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { repoDir, fixturePath } = makeTestRepo();
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(fixturePath);
    const overlay = await openOverlay({ locus, db: mainDb });

    // Worktree exists and contains the fixture file.
    expect(existsSync(overlay.worktreePath)).toBe(true);
    const scratchFixture = join(overlay.worktreePath, "fixture.ts");
    expect(existsSync(scratchFixture)).toBe(true);

    // Scratch DB is a DIFFERENT file than main DB.
    expect(overlay.sastDbPath).not.toBe(join(mainTmp, "main.db"));
    expect(existsSync(overlay.sastDbPath)).toBe(true);

    // Pre-index ran: fixture.ts is in the scratch files table.
    const fileRows = overlay.sastDb.select().from(files).all();
    expect(fileRows.length).toBeGreaterThan(0);
    expect(fileRows.some((r) => r.path.endsWith("fixture.ts"))).toBe(true);

    // Close: both worktree dir and DB file should be gone.
    await closeOverlay(overlay);
    expect(existsSync(overlay.worktreePath)).toBe(false);
    expect(existsSync(overlay.sastDbPath)).toBe(false);
    expect(overlay.closed).toBe(true);
  });

  // -------------------------------------------------------------------------
  // 2. applyPatchToOverlay modifies scratch file only, not original
  // -------------------------------------------------------------------------
  it("applyPatchToOverlay modifies scratch file but leaves original untouched", async () => {
    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-overlay-test-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { repoDir, fixturePath } = makeTestRepo();
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const originalContent = readFileSync(fixturePath, "utf8");
    const locus = makeLocus(fixturePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => { try { overlay.sastDb.$client.close(); } catch { /* ignore */ } });

    applyPatchToOverlay(overlay, {
      file: "fixture.ts",
      newContent: "export function add(a: number, b: number) { return a + b + 1; }",
      rationale: "test patch",
    });

    const scratchFixture = join(overlay.worktreePath, "fixture.ts");
    expect(readFileSync(scratchFixture, "utf8")).toContain("+ 1");
    expect(readFileSync(fixturePath, "utf8")).toBe(originalContent);

    await closeOverlay(overlay);
  });

  // -------------------------------------------------------------------------
  // 3. reindexOverlay updates scratch DB, not main DB
  // -------------------------------------------------------------------------
  it("reindexOverlay updates scratch DB; main DB retains the pre-fix state", async () => {
    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-overlay-test-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { repoDir, fixturePath } = makeTestRepo();
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const { db: mainDb, dbPath: mainDbPath } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    // Pre-index the fixture into the main DB.
    const { buildSASTForFile } = await import("../sast/builder.js");
    buildSASTForFile(mainDb, fixturePath);
    const mainNodesBefore = mainDb.select().from(nodes).all().length;

    const locus = makeLocus(fixturePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => { try { overlay.sastDb.$client.close(); } catch { /* ignore */ } });

    // Apply a patch with a new function (more nodes expected).
    const newContent = `
export function add(a: number, b: number) { return a + b; }
export function sub(a: number, b: number) { return a - b; }
export function mul(a: number, b: number) { return a * b; }
`.trim();

    applyPatchToOverlay(overlay, { file: "fixture.ts", newContent });
    await reindexOverlay(overlay);

    // Scratch DB has nodes for the new content.
    const scratchNodes = overlay.sastDb.select().from(nodes).all();
    expect(scratchNodes.length).toBeGreaterThan(0);

    // Main DB node count unchanged.
    const mainNodesAfter = mainDb.select().from(nodes).all().length;
    expect(mainNodesAfter).toBe(mainNodesBefore);

    await closeOverlay(overlay);
  });

  // -------------------------------------------------------------------------
  // 4. Multiple patches on same file accumulate in modifiedFiles as a Set
  // -------------------------------------------------------------------------
  it("multiple patches on the same file are recorded once in modifiedFiles", async () => {
    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-overlay-test-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { repoDir, fixturePath } = makeTestRepo();
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(fixturePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => { try { overlay.sastDb.$client.close(); } catch { /* ignore */ } });

    applyPatchToOverlay(overlay, { file: "fixture.ts", newContent: "// patch A\nexport const x = 1;" });
    await reindexOverlay(overlay);

    applyPatchToOverlay(overlay, { file: "fixture.ts", newContent: "// patch B\nexport const x = 2;" });
    await reindexOverlay(overlay);

    // Set deduplicates: only one entry for fixture.ts.
    expect(overlay.modifiedFiles.size).toBe(1);
    expect(overlay.modifiedFiles.has("fixture.ts")).toBe(true);

    await closeOverlay(overlay);
  });

  // -------------------------------------------------------------------------
  // 5. closeOverlay is idempotent (second close is a no-op)
  // -------------------------------------------------------------------------
  it("closeOverlay is idempotent — second close returns without error", async () => {
    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-overlay-test-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { repoDir, fixturePath } = makeTestRepo();
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(fixturePath);
    const overlay = await openOverlay({ locus, db: mainDb });

    await closeOverlay(overlay);
    // Second close must not throw.
    await expect(closeOverlay(overlay)).resolves.toBeUndefined();
  });

  // -------------------------------------------------------------------------
  // 6. Overlay DOES NOT touch main DB
  // -------------------------------------------------------------------------
  it("overlay operations never modify the main DB", async () => {
    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-overlay-test-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { repoDir, fixturePath } = makeTestRepo();
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    // Seed main DB with the fixture.
    const { buildSASTForFile } = await import("../sast/builder.js");
    buildSASTForFile(mainDb, fixturePath);

    const mainRow = mainDb.select().from(files).where(eq(files.path, fixturePath)).get();
    expect(mainRow).toBeDefined();
    const { id: mainId, contentHash: mainHash, parsedAt: mainParsedAt } = mainRow!;

    const locus = makeLocus(fixturePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => { try { overlay.sastDb.$client.close(); } catch { /* ignore */ } });

    applyPatchToOverlay(overlay, {
      file: "fixture.ts",
      newContent: "export function changed() { return 42; }",
    });
    await reindexOverlay(overlay);

    // Main DB row must be byte-for-byte identical.
    const mainRowAfter = mainDb.select().from(files).where(eq(files.path, fixturePath)).get();
    expect(mainRowAfter).toBeDefined();
    expect(mainRowAfter!.id).toBe(mainId);
    expect(mainRowAfter!.contentHash).toBe(mainHash);
    expect(mainRowAfter!.parsedAt).toBe(mainParsedAt);

    await closeOverlay(overlay);
  });

  // -------------------------------------------------------------------------
  // 7. Error path: non-git directory — throws, leaves no scratch resources
  // -------------------------------------------------------------------------
  it("openOverlay throws a clear error when locus file is not in a git repo", async () => {
    const nonGitDir = mkdtempSync(join(tmpdir(), "provekit-overlay-nongit-"));
    cleanups.push(() => rmSync(nonGitDir, { recursive: true, force: true }));

    const fakePath = join(nonGitDir, "src.ts");
    writeFileSync(fakePath, "const x = 1;", "utf8");

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-overlay-test-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(fakePath);
    await expect(openOverlay({ locus, db: mainDb })).rejects.toThrow(
      /not inside a git repository/,
    );

    // No scratch directories should have been created.
    const { readdirSync } = await import("fs");
    const strayDirs = readdirSync(tmpdir()).filter((n) =>
      n.startsWith("provekit-overlay-") && !n.startsWith("provekit-overlay-nongit") && !n.startsWith("provekit-overlay-test"),
    );
    expect(strayDirs).toHaveLength(0);
  });
});
