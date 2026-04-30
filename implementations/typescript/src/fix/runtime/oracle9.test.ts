/**
 * Tests for src/fix/runtime/oracle9.ts.
 *
 * verifyOracle9 has three success/failure shapes worth pinning:
 *   1. No-runner sentinel: skip the whole oracle and return a synthesized
 *      pass shape. (No git repo needed; testRunner short-circuits before
 *      revertFixInOverlay runs.)
 *   2. Oracle #9a fail: test does not pass against the fixed code → throws.
 *      (Same short-circuit — never reaches revert.)
 *   3. Oracle #9b fail: test PASSES against the original (unfixed) code,
 *      meaning the test is a placebo. Requires a real git fixture so
 *      revertFixInOverlay can call `git show HEAD:<file>`.
 *   4. Happy path: passes on fixed, fails on original. Same git fixture.
 *
 * The test injects a stub testRunner so we never spawn real vitest.
 * applyPatch and reindex are no-ops in the unit setup.
 */
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, readFileSync, mkdirSync } from "fs";
import { execFileSync } from "child_process";
import { tmpdir } from "os";
import { join } from "path";
import type { OverlayHandle, FixCandidate } from "../types.js";
import type { Db } from "../../db/index.js";
import { verifyOracle9 } from "./oracle9.js";

const GIT_ID = ["-c", "user.name=t", "-c", "user.email=t@t.test"];

let dirs: string[] = [];

afterEach(() => {
  for (const d of dirs) {
    try {
      rmSync(d, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
  dirs = [];
});

function makeOverlayDir(): string {
  const d = mkdtempSync(join(tmpdir(), "oracle9-"));
  dirs.push(d);
  return d;
}

function makeOverlay(worktreePath: string): OverlayHandle {
  return {
    worktreePath,
    sastDbPath: join(worktreePath, ".provekit", "scratch.db"),
    sastDb: {} as unknown as Db,
    baseRef: "HEAD",
    modifiedFiles: new Set<string>(),
    closed: false,
  };
}

function makeFix(file: string, fixedContent: string): FixCandidate {
  return {
    patch: {
      fileEdits: [{ file, newContent: fixedContent }],
      description: "fix",
    },
    source: "llm",
    llmRationale: "",
    llmConfidence: 1,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 0,
      overlayClosed: false,
    },
  };
}

function initRepo(dir: string, files: Record<string, string>) {
  execFileSync("git", [...GIT_ID, "init", "-q", "-b", "main"], { cwd: dir });
  for (const [rel, content] of Object.entries(files)) {
    const abs = join(dir, rel);
    mkdirSync(join(abs, ".."), { recursive: true });
    writeFileSync(abs, content, "utf8");
  }
  execFileSync("git", [...GIT_ID, "add", "."], { cwd: dir });
  execFileSync("git", [...GIT_ID, "commit", "-q", "-m", "init"], { cwd: dir });
}

describe("verifyOracle9 — runner sentinel skips the whole oracle", () => {
  it("returns a synthesized pass when the runner returns the no-runner sentinel", async () => {
    const dir = makeOverlayDir();
    const overlay = makeOverlay(dir);
    const fix = makeFix("src/foo.ts", "fixed");

    const sentinel = "no test runner; oracle #9 skipped (no node_modules in main repo)";
    const result = await verifyOracle9({
      overlay,
      fix,
      testFilePath: "src/foo.test.ts",
      testCode: "// stub",
      mainRepoRoot: dir,
      testRunner: () => ({ exitCode: 0, stdout: sentinel, stderr: "" }),
      applyPatch: () => {
        /* noop */
      },
      reindex: async () => {
        /* noop */
      },
    });

    expect(result.passesOnFixedCode).toBe(true);
    expect(result.failsOnOriginalCode).toBe(true);
    expect(result.audit.mutationApplied).toBe(false);
    expect(result.audit.mutationReverted).toBe(false);
    expect(result.audit.fixedRunStdout).toBe(sentinel);
  });
});

describe("verifyOracle9 — Oracle #9a (test must pass against fixed code)", () => {
  it("throws when the runner returns a non-zero exit code on the fixed run", async () => {
    const dir = makeOverlayDir();
    const overlay = makeOverlay(dir);
    const fix = makeFix("src/foo.ts", "fixed");

    await expect(
      verifyOracle9({
        overlay,
        fix,
        testFilePath: "src/foo.test.ts",
        testCode: "// stub",
        mainRepoRoot: dir,
        testRunner: () => ({
          exitCode: 1,
          stdout: "AssertionError",
          stderr: "boom",
        }),
        applyPatch: () => {
          /* noop */
        },
        reindex: async () => {
          /* noop */
        },
      }),
    ).rejects.toThrow(/oracle #9a FAIL/);
  });
});

describe("verifyOracle9 — Oracle #9b (mutation check) requires a git repo", () => {
  it("returns a happy result when test passes on fixed and fails on reverted", async () => {
    const dir = makeOverlayDir();
    initRepo(dir, { "src/foo.ts": "function f() { return 1; }\n" });
    const overlay = makeOverlay(dir);
    const fix = makeFix("src/foo.ts", "function f() { return 2; /* fixed */ }\n");

    // Apply patch: write new content (this is what applyPatchToOverlay does
    // by default — the unit test does it eagerly so the fixed run has the
    // correct file on disk).
    writeFileSync(join(dir, "src/foo.ts"), fix.patch.fileEdits[0].newContent, "utf8");

    let runCount = 0;
    const result = await verifyOracle9({
      overlay,
      fix,
      testFilePath: "src/foo.regression.test.ts",
      testCode: "import { it } from 'vitest';\nit('placebo', () => {});",
      mainRepoRoot: dir,
      // First call: fixed run → pass. Second call: reverted run → fail.
      testRunner: () => {
        runCount++;
        if (runCount === 1) return { exitCode: 0, stdout: "PASS", stderr: "" };
        return { exitCode: 1, stdout: "FAIL", stderr: "" };
      },
      applyPatch: () => {
        /* test file already exists in fixture / not needed for the runner stub */
      },
      reindex: async () => {
        /* noop */
      },
    });

    expect(result.passesOnFixedCode).toBe(true);
    expect(result.failsOnOriginalCode).toBe(true);
    expect(result.audit.mutationApplied).toBe(true);
    expect(result.audit.mutationReverted).toBe(true);
    expect(result.audit.fixedRunExitCode).toBe(0);
    expect(result.audit.originalRunExitCode).toBe(1);

    // Restore step ran: the fixed content should be back on disk.
    const after = readFileSync(join(dir, "src/foo.ts"), "utf8");
    expect(after).toBe(fix.patch.fileEdits[0].newContent);
  });

  it("throws on Oracle #9b fail (test PASSES against unfixed code → placebo test)", async () => {
    const dir = makeOverlayDir();
    initRepo(dir, { "src/foo.ts": "function f() { return 1; }\n" });
    const overlay = makeOverlay(dir);
    const fix = makeFix("src/foo.ts", "function f() { return 2; }\n");
    writeFileSync(join(dir, "src/foo.ts"), fix.patch.fileEdits[0].newContent, "utf8");

    await expect(
      verifyOracle9({
        overlay,
        fix,
        testFilePath: "src/foo.regression.test.ts",
        testCode: "// placebo",
        mainRepoRoot: dir,
        // Both runs pass — the test is a placebo (does not lock in the fix).
        testRunner: () => ({ exitCode: 0, stdout: "PASS", stderr: "" }),
        applyPatch: () => {
          /* noop */
        },
        reindex: async () => {
          /* noop */
        },
      }),
    ).rejects.toThrow(/oracle #9b FAIL/);

    // After throw, the helper restores the fix (best-effort) so the overlay
    // is in a clean state for callers that want to inspect it.
    const after = readFileSync(join(dir, "src/foo.ts"), "utf8");
    expect(after).toBe(fix.patch.fileEdits[0].newContent);
  });
});
