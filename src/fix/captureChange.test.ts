/**
 * captureChange.test.ts — tests for runAgentInOverlay + reconstructCodePatch.
 *
 * Uses a real git tempdir to verify that the agent path correctly captures
 * file changes via git diff after the stub agent writes files.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { runAgentInOverlay, getChangedFiles } from "./captureChange.js";
import { openOverlay } from "./stages/openOverlay.js";
import { closeOverlay } from "./overlay.js";
import { StubLLMProvider, OverlayBypassError } from "./types.js";
import type { BugLocus } from "./types.js";

const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];

function makeTestRepo(content: string, filename = "fixture.ts"): { repoDir: string; filePath: string } {
  const repoDir = mkdtempSync(join(tmpdir(), "provekit-cc-test-repo-"));
  execFileSync("git", [...GIT_ID, "init", repoDir]);
  execFileSync("git", [...GIT_ID, "init"], { cwd: repoDir });

  const filePath = join(repoDir, filename);
  writeFileSync(filePath, content, "utf8");

  execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
  execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

  return { repoDir, filePath };
}

function openMainDb(dir: string) {
  const dbPath = join(dir, "main.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, dbPath };
}

function makeLocus(filePath: string): BugLocus {
  return {
    file: filePath,
    line: 1,
    confidence: 1.0,
    primaryNode: "aaaa000000000000",
    containingFunction: "aaaa000000000000",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };
}

describe("captureChange: runAgentInOverlay", () => {
  const cleanups: (() => void | Promise<void>)[] = [];

  afterEach(async () => {
    for (const fn of cleanups.splice(0)) {
      try { await fn(); } catch { /* ignore */ }
    }
  });

  // -------------------------------------------------------------------------
  // 1. Single file edit round-trip
  // -------------------------------------------------------------------------
  it("stub agent edits one file → reconstructCodePatch recovers new content", async () => {
    const source = "export function foo() { return 1; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-maindb-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const fixedContent = "export function foo() { return 42; }\n";
    const llm = new StubLLMProvider(
      new Map(),
      [{ matchPrompt: "fix", fileEdits: [{ file: "fixture.ts", newContent: fixedContent }], text: "Changed return to 42" }],
    );

    const { patch, rationale, turnsUsed } = await runAgentInOverlay({
      overlay,
      llm,
      prompt: "Please fix the issue",
    });

    expect(patch.fileEdits).toHaveLength(1);
    expect(patch.fileEdits[0]!.file).toBe("fixture.ts");
    expect(patch.fileEdits[0]!.newContent).toBe(fixedContent);
    expect(rationale).toBe("Changed return to 42");
    expect(turnsUsed).toBe(1);
  }, 30_000);

  // -------------------------------------------------------------------------
  // 2. Two file edits round-trip
  // -------------------------------------------------------------------------
  it("stub agent edits 2 files → reconstructCodePatch recovers both", async () => {
    const source = "export function bar() { return 'original'; }\n";
    const { repoDir, filePath } = makeTestRepo(source, "a.ts");
    // Add a second file to the repo.
    const secondFilePath = join(repoDir, "b.ts");
    writeFileSync(secondFilePath, "export const X = 0;\n", "utf8");
    execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
    execFileSync("git", [...GIT_ID, "commit", "-m", "add b.ts"], { cwd: repoDir });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-maindb2-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const fixedA = "export function bar() { return 'fixed'; }\n";
    const fixedB = "export const X = 99;\n";
    const llm = new StubLLMProvider(
      new Map(),
      [{
        matchPrompt: "fix",
        fileEdits: [
          { file: "a.ts", newContent: fixedA },
          { file: "b.ts", newContent: fixedB },
        ],
        text: "Fixed both files",
      }],
    );

    const { patch, turnsUsed } = await runAgentInOverlay({
      overlay,
      llm,
      prompt: "Please fix both files",
    });

    expect(patch.fileEdits).toHaveLength(2);
    const files = patch.fileEdits.map((e) => e.file).sort();
    expect(files).toEqual(["a.ts", "b.ts"]);
    const aEdit = patch.fileEdits.find((e) => e.file === "a.ts")!;
    const bEdit = patch.fileEdits.find((e) => e.file === "b.ts")!;
    expect(aEdit.newContent).toBe(fixedA);
    expect(bEdit.newContent).toBe(fixedB);
    expect(turnsUsed).toBe(1);
  }, 30_000);

  // -------------------------------------------------------------------------
  // 3. Provider without agent() throws
  // -------------------------------------------------------------------------
  it("throws if LLM provider has no agent() method", async () => {
    const source = "export function baz() { return 0; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-maindb3-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    // No agentResponses → no agent() method.
    const llm = new StubLLMProvider(new Map());

    await expect(
      runAgentInOverlay({ overlay, llm, prompt: "fix it" }),
    ).rejects.toThrow(/does not implement agent\(\)/);
  }, 15_000);

  // -------------------------------------------------------------------------
  // 4. Layer 2: stub agent uses Edit outside cwd → OverlayBypassError
  // -------------------------------------------------------------------------
  it("throws OverlayBypassError when stub agent reports Edit on absolute path outside overlay", async () => {
    const source = "export function qux() { return 1; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-maindb4-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    // Stub agent writes inside cwd (correct), but also reports a tool_use that
    // touched a path outside the overlay (the bypass).
    const outsidePath = filePath; // original repo file — outside the overlay

    const llm = new StubLLMProvider(
      new Map(),
      [{
        matchPrompt: "fix",
        fileEdits: [{ file: "fixture.ts", newContent: "export function qux() { return 99; }\n" }],
        text: "Fixed",
        toolUses: [
          {
            id: "tu-bypass",
            name: "Edit",
            input: { file_path: outsidePath, old_string: "return 1", new_string: "return 99" },
            result: "ok",
            isError: false,
            turn: 1,
            ms: 10,
          },
        ],
      }],
    );

    await expect(
      runAgentInOverlay({ overlay, llm, prompt: "please fix" }),
    ).rejects.toThrow(OverlayBypassError);
  }, 30_000);

  // -------------------------------------------------------------------------
  // 5. Layer 2: stub agent uses Edit inside overlay → no throw
  // -------------------------------------------------------------------------
  it("does NOT throw when stub agent reports Edit on path inside overlay", async () => {
    const source = "export function quux() { return 1; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-maindb5-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    // The tool_use path is inside the overlay — this should be allowed.
    const insidePath = join(overlay.worktreePath, "fixture.ts");

    const fixedContent = "export function quux() { return 42; }\n";
    const llm = new StubLLMProvider(
      new Map(),
      [{
        matchPrompt: "fix",
        fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
        text: "Fixed",
        toolUses: [
          {
            id: "tu-ok",
            name: "Edit",
            input: { file_path: insidePath, old_string: "return 1", new_string: "return 42" },
            result: "ok",
            isError: false,
            turn: 1,
            ms: 10,
          },
        ],
      }],
    );

    const { patch } = await runAgentInOverlay({ overlay, llm, prompt: "please fix" });
    expect(patch.fileEdits).toHaveLength(1);
    expect(patch.fileEdits[0]!.newContent).toBe(fixedContent);
  }, 30_000);

  // -------------------------------------------------------------------------
  // 5b. Layer 2: stub agent hallucinates absolute Write path then self-corrects
  // -------------------------------------------------------------------------
  it("does NOT throw on Write bypass when same .provekit tail was also written inside overlay (self-correction)", async () => {
    // Bug-1 v16 regression: agent hallucinated /home/user/.provekit/...
    // for the first round of writes, then ran pwd, saw the real overlay,
    // and re-wrote everything under /<overlay>/.provekit/...  The bypass
    // detector caught the first write and threw, even though the agent
    // had already self-corrected. This tolerates that case.
    const source = "export function corge() { return 1; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-selfcorr-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const tailA = ".provekit/capability-proposal/foo/schema.ts";
    const tailB = ".provekit/capability-proposal/foo/migration.sql";
    const llm = new StubLLMProvider(
      new Map(),
      [{
        matchPrompt: "fix",
        fileEdits: [
          { file: tailA, newContent: "// schema\n" },
          { file: tailB, newContent: "CREATE TABLE x (id TEXT)\n" },
        ],
        text: "Wrote files",
        toolUses: [
          // First — agent hallucinates absolute /home/user/ path
          {
            id: "tu-bad-1",
            name: "Write",
            input: { file_path: "/home/user/" + tailA, content: "// schema\n" },
            result: "ok",
            isError: false,
            turn: 1,
            ms: 10,
          },
          {
            id: "tu-bad-2",
            name: "Write",
            input: { file_path: "/home/user/" + tailB, content: "CREATE TABLE x (id TEXT)\n" },
            result: "ok",
            isError: false,
            turn: 1,
            ms: 10,
          },
          // Then — agent corrects to the real overlay path
          {
            id: "tu-good-1",
            name: "Write",
            input: { file_path: join(overlay.worktreePath, tailA), content: "// schema\n" },
            result: "ok",
            isError: false,
            turn: 2,
            ms: 10,
          },
          {
            id: "tu-good-2",
            name: "Write",
            input: { file_path: join(overlay.worktreePath, tailB), content: "CREATE TABLE x (id TEXT)\n" },
            result: "ok",
            isError: false,
            turn: 2,
            ms: 10,
          },
        ],
      }],
    );

    // Should NOT throw — Write bypass was self-corrected.
    const { patch } = await runAgentInOverlay({ overlay, llm, prompt: "fix" });
    expect(patch).toBeDefined();
  }, 30_000);

  it("DOES throw on Edit bypass even if a corresponding overlay-internal write exists", async () => {
    // Edit on an outside path means modifying real existing files —
    // self-correction via a separate Write doesn't undo the Edit. Always throw.
    const source = "export function grault() { return 1; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-edit-bypass-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const tail = ".provekit/principles/foo.dsl";
    const llm = new StubLLMProvider(
      new Map(),
      [{
        matchPrompt: "fix",
        fileEdits: [{ file: tail, newContent: "principle\n" }],
        text: "Fixed",
        toolUses: [
          {
            id: "tu-edit-bypass",
            name: "Edit",
            input: { file_path: "/home/user/" + tail, old_string: "x", new_string: "y" },
            result: "ok",
            isError: false,
            turn: 1,
            ms: 10,
          },
          {
            id: "tu-write-good",
            name: "Write",
            input: { file_path: join(overlay.worktreePath, tail), content: "principle\n" },
            result: "ok",
            isError: false,
            turn: 1,
            ms: 10,
          },
        ],
      }],
    );

    await expect(
      runAgentInOverlay({ overlay, llm, prompt: "fix" }),
    ).rejects.toThrow(OverlayBypassError);
  }, 30_000);

  // -------------------------------------------------------------------------
  // 6. Layer 2: stub agent uses Bash with absolute path outside overlay → warn (no throw)
  // -------------------------------------------------------------------------
  it("does NOT throw (only warns) when stub agent Bash command references path outside overlay", async () => {
    const source = "export function corge() { return 1; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-cc-maindb6-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const fixedContent = "export function corge() { return 7; }\n";
    const llm = new StubLLMProvider(
      new Map(),
      [{
        matchPrompt: "fix",
        fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
        text: "Fixed",
        toolUses: [
          {
            id: "tu-bash",
            name: "Bash",
            // Bash command that references an absolute path outside the overlay.
            input: { command: `cat ${filePath}` },
            result: "export function corge() { return 1; }",
            isError: false,
            turn: 1,
            ms: 5,
          },
        ],
      }],
    );

    // Should NOT throw — Bash bypass is warn-only (too many false positives).
    const { patch } = await runAgentInOverlay({ overlay, llm, prompt: "please fix" });
    expect(patch.fileEdits).toHaveLength(1);
  }, 30_000);
});
