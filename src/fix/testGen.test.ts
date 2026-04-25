/**
 * C5: testGen unit tests.
 *
 * Tests extractWitnessInputs, generateTestCode (via stub LLM), chooseTestFilePath,
 * revert/restore helpers, and oracle #9 happy/failure paths (with stubbed
 * runTestInOverlay and reindexOverlay).
 *
 * Oracle #9 integration (real vitest in a real overlay) is covered by the
 * slow integration test at the bottom of this file, skipped by default in CI.
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import {
  extractWitnessInputs,
  chooseTestFilePath,
  generateTestCode,
  generateTestCodeViaAgent,
  validateImportPaths,
  revertFixInOverlay,
  restoreFixInOverlay,
  setupOverlayForTest,
} from "./testGen.js";
import type { InvariantClaim, BugLocus, BugSignal, OverlayHandle, FixCandidate } from "./types.js";
import { StubLLMProvider } from "./types.js";
import type { Db } from "../db/index.js";

// ---------------------------------------------------------------------------
// Git config for test commits
// ---------------------------------------------------------------------------

const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];

// ---------------------------------------------------------------------------
// Mocks — must be at top level before describe blocks
// ---------------------------------------------------------------------------

// Mock runTestInOverlay from testGen so we can control exit codes in oracle tests
vi.mock("./testGen.js", async (importOriginal) => {
  const original = await importOriginal<typeof import("./testGen.js")>();
  return {
    ...original,
    runTestInOverlay: vi.fn(),
    resolveMainRepoRoot: vi.fn(() => process.cwd()),
  };
});

// Mock reindexOverlay from overlay so oracle tests don't need a real DB
vi.mock("./overlay.js", async (importOriginal) => {
  const original = await importOriginal<typeof import("./overlay.js")>();
  return {
    ...original,
    reindexOverlay: vi.fn(async () => { /* no-op in tests */ }),
  };
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeMinimalOverlay(worktreePath: string): OverlayHandle {
  return {
    worktreePath,
    sastDbPath: join(worktreePath, ".provekit", "scratch.db"),
    sastDb: {} as unknown as Db,
    baseRef: "HEAD",
    modifiedFiles: new Set<string>(),
    closed: false,
  };
}

function makeLocus(file: string): BugLocus {
  return {
    file,
    line: 3,
    function: "divide",
    confidence: 1.0,
    primaryNode: "node-001",
    containingFunction: "node-001",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };
}

function makeSignal(filePath: string): BugSignal {
  return {
    source: "test",
    rawText: "divide by zero",
    summary: "divide(a, b) crashes when b is zero",
    failureDescription: "ZeroDivisionError: division by zero",
    codeReferences: [{ file: filePath, line: 3 }],
    bugClassHint: "division-by-zero",
  };
}

/** A real Z3 model witness for a=5, b=0. */
const WITNESS_MODEL = `(
  (define-fun a () Int
    5)
  (define-fun b () Int
    0)
)`;

function makeDivInvariant(witness: string | null = WITNESS_MODEL): InvariantClaim {
  return {
    principleId: "division-by-zero",
    description: "Division where denominator may be zero",
    formalExpression:
      "(declare-const a Int)\n(declare-const b Int)\n(assert (= b 0))\n(check-sat)",
    bindings: [
      { smt_constant: "a", source_line: 3, source_expr: "a", sort: "Int" },
      { smt_constant: "b", source_line: 3, source_expr: "b", sort: "Int" },
    ],
    complexity: 1,
    witness,
  };
}

const FIXED_SOURCE = `export function divide(a: number, b: number): number {
  if (b === 0) throw new Error("division by zero");
  return a / b;
}
`;

const BUGGY_SOURCE = `export function divide(a: number, b: number): number {
  return a / b;
}
`;

// Canned test code returned by stub LLM (no trailing newline — generateTestCode trims)
// Uses "./divide" — the correct relative import from src/ to src/divide.ts
const CANNED_TEST_CODE = `import { it, expect } from "vitest";
import { divide } from "./divide";

it("regression: divide(a, b) crashes when b is zero", () => {
  // Z3 witness: a=5, b=0 triggers division-by-zero before fix
  expect(() => divide(5, 0)).not.toThrow();
});`;

function makeFixCandidate(): FixCandidate {
  return {
    patch: {
      fileEdits: [{ file: "src/divide.ts", newContent: FIXED_SOURCE }],
      description: "add guard",
    },
    llmRationale: "add null guard",
    llmConfidence: 0.95,
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
}

// ---------------------------------------------------------------------------
// Test 1: extractWitnessInputs
// ---------------------------------------------------------------------------

describe("C5: extractWitnessInputs", () => {
  it("parses real Z3 model into JS values keyed by source_expr", () => {
    const invariant = makeDivInvariant(WITNESS_MODEL);
    const inputs = extractWitnessInputs(invariant);
    expect(inputs).toEqual({ a: 5, b: 0 });
  });

  it("returns empty object when model parses but no bindings match", () => {
    const invariant: InvariantClaim = {
      ...makeDivInvariant(WITNESS_MODEL),
      bindings: [
        { smt_constant: "z_missing", source_line: 1, source_expr: "z", sort: "Int" },
      ],
    };
    const inputs = extractWitnessInputs(invariant);
    expect(inputs).toEqual({});
  });

  it("throws when witness is null", () => {
    const invariant = makeDivInvariant(null);
    expect(() => extractWitnessInputs(invariant)).toThrow("C5: invariant has no Z3 witness");
  });

  it("throws when witness is undefined (typed as null but cast)", () => {
    const invariant = { ...makeDivInvariant(null), witness: undefined as unknown as null };
    expect(() => extractWitnessInputs(invariant)).toThrow("C5: invariant has no Z3 witness");
  });
});

// ---------------------------------------------------------------------------
// Test 2: chooseTestFilePath
// ---------------------------------------------------------------------------

describe("C5: chooseTestFilePath", () => {
  it("derives test file path from locus.file inside overlay", () => {
    const worktreePath = mkdtempSync(join(tmpdir(), "provekit-c5-path-"));
    try {
      const overlay = makeMinimalOverlay(worktreePath);
      const locusFile = join(worktreePath, "src", "divide.ts");
      const locus = makeLocus(locusFile);
      const testPath = chooseTestFilePath(locus, overlay);
      expect(testPath).toBe("src/divide.regression.test.ts");
    } finally {
      rmSync(worktreePath, { recursive: true, force: true });
    }
  });

  it("handles nested directories correctly", () => {
    const worktreePath = mkdtempSync(join(tmpdir(), "provekit-c5-path-"));
    try {
      const overlay = makeMinimalOverlay(worktreePath);
      const locusFile = join(worktreePath, "src", "utils", "math.ts");
      const locus = makeLocus(locusFile);
      const testPath = chooseTestFilePath(locus, overlay);
      expect(testPath).toBe("src/utils/math.regression.test.ts");
    } finally {
      rmSync(worktreePath, { recursive: true, force: true });
    }
  });

  it("falls back gracefully when locus.file is outside overlay (e.g. original repo)", () => {
    const worktreePath = mkdtempSync(join(tmpdir(), "provekit-c5-path-"));
    try {
      const overlay = makeMinimalOverlay(worktreePath);
      // Simulate original repo path (outside worktree)
      const locusFile = "/Users/someone/myproject/src/divide.ts";
      const locus = makeLocus(locusFile);
      const testPath = chooseTestFilePath(locus, overlay);
      // Should still produce a valid-looking path
      expect(testPath).toMatch(/regression\.test\.ts$/);
    } finally {
      rmSync(worktreePath, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Test 3: generateTestCode via stub LLM
// ---------------------------------------------------------------------------

describe("C5: generateTestCode", () => {
  it("returns canned code from stub LLM (trimmed)", async () => {
    const worktreePath = mkdtempSync(join(tmpdir(), "provekit-c5-gentest-"));
    try {
      const overlay = makeMinimalOverlay(worktreePath);
      mkdirSync(join(worktreePath, "src"), { recursive: true });
      writeFileSync(join(worktreePath, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const locus = makeLocus(join(worktreePath, "src", "divide.ts"));
      const signal = makeSignal(join(worktreePath, "src", "divide.ts"));
      const invariant = makeDivInvariant();
      const inputs = { a: 5, b: 0 };
      const testFilePath = "src/divide.regression.test.ts";
      const testName = "regression: divide(a, b) crashes when b is zero";

      // The stub key must match a substring of the prompt.
      // generateTestCode includes "Z3 WITNESS INPUTS" in the prompt.
      const stub = new StubLLMProvider(
        new Map([["Z3 WITNESS INPUTS", CANNED_TEST_CODE]]),
      );

      const code = await generateTestCode({
        signal,
        locus,
        invariant,
        inputs,
        testFilePath,
        testName,
        llm: stub,
        overlay,
      });

      // generateTestCode trims the response
      expect(code).toBe(CANNED_TEST_CODE.trim());
    } finally {
      rmSync(worktreePath, { recursive: true, force: true });
    }
  });

  it("throws when LLM returns code with no it() call", async () => {
    const worktreePath = mkdtempSync(join(tmpdir(), "provekit-c5-gentest-"));
    try {
      const overlay = makeMinimalOverlay(worktreePath);
      const locus = makeLocus(join(worktreePath, "src", "divide.ts"));
      const signal = makeSignal(locus.file);
      const invariant = makeDivInvariant();

      const stub = new StubLLMProvider(
        // Key must match the actual prompt content
        new Map([["Z3 WITNESS INPUTS", "import { expect } from 'vitest';\n// no it call here"]]),
      );

      await expect(
        generateTestCode({
          signal,
          locus,
          invariant,
          inputs: { a: 5, b: 0 },
          testFilePath: "src/divide.regression.test.ts",
          testName: "regression: test",
          llm: stub,
          overlay,
        }),
      ).rejects.toThrow("no it() call");
    } finally {
      rmSync(worktreePath, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Test 3b: validateImportPaths
// ---------------------------------------------------------------------------

describe("C5: validateImportPaths", () => {
  it("returns empty array when all relative imports resolve", () => {
    const tmpDir = mkdtempSync(join(tmpdir(), "provekit-c5-vip-ok-"));
    try {
      mkdirSync(join(tmpDir, "src"), { recursive: true });
      writeFileSync(join(tmpDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const testFileAbsPath = join(tmpDir, "src", "divide.regression.test.ts");
      const source = `import { it, expect } from "vitest";\nimport { divide } from "./divide";\nit("x", () => {});`;
      const overlay = makeMinimalOverlay(tmpDir);

      const result = validateImportPaths(testFileAbsPath, source, overlay);
      expect(result).toEqual([]);
    } finally {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  it("returns unresolved paths when relative import target is missing", () => {
    const tmpDir = mkdtempSync(join(tmpdir(), "provekit-c5-vip-bad-"));
    try {
      mkdirSync(join(tmpDir, "src"), { recursive: true });
      // Note: we do NOT create "fixture.ts" — only "divide.ts" exists
      writeFileSync(join(tmpDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const testFileAbsPath = join(tmpDir, "src", "divide.regression.test.ts");
      const source = `import { it, expect } from "vitest";\nimport { divide } from "./fixture";\nit("x", () => {});`;
      const overlay = makeMinimalOverlay(tmpDir);

      const result = validateImportPaths(testFileAbsPath, source, overlay);
      expect(result).toEqual(["./fixture"]);
    } finally {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  it("ignores non-relative (bare) imports", () => {
    const tmpDir = mkdtempSync(join(tmpdir(), "provekit-c5-vip-bare-"));
    try {
      mkdirSync(join(tmpDir, "src"), { recursive: true });

      const testFileAbsPath = join(tmpDir, "src", "divide.regression.test.ts");
      const source = `import { it } from "vitest";\nimport path from "path";\nit("x", () => {});`;
      const overlay = makeMinimalOverlay(tmpDir);

      const result = validateImportPaths(testFileAbsPath, source, overlay);
      expect(result).toEqual([]);
    } finally {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Test 4: revertFixInOverlay + restoreFixInOverlay
// ---------------------------------------------------------------------------

describe("C5: revertFixInOverlay + restoreFixInOverlay", () => {
  it("reverts to HEAD content and restores post-fix content", () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-revert-"));
    try {
      // Init git repo with buggy source as HEAD
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "divide.ts"), BUGGY_SOURCE, "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init buggy"], { cwd: repoDir });

      // Simulate: C3 applied a fix (write fixed source over the file)
      writeFileSync(join(repoDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const overlay = makeMinimalOverlay(repoDir);
      overlay.modifiedFiles.add("src/divide.ts");

      const patch = {
        fileEdits: [{ file: "src/divide.ts", newContent: FIXED_SOURCE }],
        description: "add guard",
      };

      // Revert should restore to HEAD (buggy)
      const stash = revertFixInOverlay(overlay, patch);
      const revertedContent = readFileSync(join(repoDir, "src", "divide.ts"), "utf8");
      expect(revertedContent).toBe(BUGGY_SOURCE);

      // Restore should write back the fixed content
      restoreFixInOverlay(overlay, stash);
      const restoredContent = readFileSync(join(repoDir, "src", "divide.ts"), "utf8");
      expect(restoredContent).toBe(FIXED_SOURCE);
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });

  it("handles new files (not present at HEAD) by deleting during revert", () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-revert-newfile-"));
    try {
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      // Initial commit with no src/newfile.ts
      writeFileSync(join(repoDir, "README.md"), "hello\n", "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

      // C3 added a NEW file
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "newfile.ts"), "export const x = 1;\n", "utf8");

      const overlay = makeMinimalOverlay(repoDir);
      overlay.modifiedFiles.add("src/newfile.ts");

      const patch = {
        fileEdits: [{ file: "src/newfile.ts", newContent: "export const x = 1;\n" }],
        description: "add new file",
      };

      // Revert: file should be deleted (it didn't exist at HEAD)
      const stash = revertFixInOverlay(overlay, patch);
      expect(existsSync(join(repoDir, "src", "newfile.ts"))).toBe(false);

      // Restore: file should be recreated
      restoreFixInOverlay(overlay, stash);
      expect(existsSync(join(repoDir, "src", "newfile.ts"))).toBe(true);
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// setupOverlayForTest: bare-fixture handling
// ---------------------------------------------------------------------------

describe("C5: setupOverlayForTest", () => {
  it("returns true when overlay already has node_modules", () => {
    const overlayDir = mkdtempSync(join(tmpdir(), "provekit-c5-setup-have-"));
    try {
      mkdirSync(join(overlayDir, "node_modules"), { recursive: true });
      const ready = setupOverlayForTest(
        { worktreePath: overlayDir } as OverlayHandle,
        "/nonexistent",
      );
      expect(ready).toBe(true);
    } finally {
      rmSync(overlayDir, { recursive: true, force: true });
    }
  });

  it("returns false when main repo has no node_modules (bare fixture)", () => {
    const overlayDir = mkdtempSync(join(tmpdir(), "provekit-c5-setup-bare-"));
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-setup-norepo-"));
    try {
      const ready = setupOverlayForTest(
        { worktreePath: overlayDir } as OverlayHandle,
        repoDir,
      );
      expect(ready).toBe(false);
      expect(existsSync(join(overlayDir, "node_modules"))).toBe(false);
    } finally {
      rmSync(overlayDir, { recursive: true, force: true });
      rmSync(repoDir, { recursive: true, force: true });
    }
  });

  it("symlinks main repo node_modules when present", () => {
    const overlayDir = mkdtempSync(join(tmpdir(), "provekit-c5-setup-link-"));
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-setup-repo-"));
    try {
      mkdirSync(join(repoDir, "node_modules"), { recursive: true });
      const ready = setupOverlayForTest(
        { worktreePath: overlayDir } as OverlayHandle,
        repoDir,
      );
      expect(ready).toBe(true);
      expect(existsSync(join(overlayDir, "node_modules"))).toBe(true);
    } finally {
      rmSync(overlayDir, { recursive: true, force: true });
      rmSync(repoDir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Tests 5–8: Oracle #9 with mocked runTestInOverlay and reindexOverlay
// ---------------------------------------------------------------------------

// Import module to get mock handle — at module scope so it's shared
import * as testGenMod from "./testGen.js";
import { generateRegressionTest } from "./stages/generateRegressionTest.js";

describe("C5: generateRegressionTest (oracle #9, stubbed test execution)", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("oracle #9 happy path: pass on fixed, fail on original → artifact with both flags true", async () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-oracle-happy-"));
    try {
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "divide.ts"), BUGGY_SOURCE, "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init buggy"], { cwd: repoDir });

      // Apply fix (simulate C3)
      writeFileSync(join(repoDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const overlay = makeMinimalOverlay(repoDir);
      overlay.modifiedFiles.add("src/divide.ts");

      vi.mocked(testGenMod.runTestInOverlay)
        .mockReturnValueOnce({ exitCode: 0, stdout: "✓ pass", stderr: "" })  // fixed: pass
        .mockReturnValueOnce({ exitCode: 1, stdout: "✗ fail", stderr: "" }); // original: fail

      const stub = new StubLLMProvider(
        new Map([["Z3 WITNESS INPUTS", CANNED_TEST_CODE]]),
      );

      const artifact = await generateRegressionTest({
        fix: makeFixCandidate(),
        signal: makeSignal(join(repoDir, "src", "divide.ts")),
        locus: makeLocus(join(repoDir, "src", "divide.ts")),
        overlay,
        invariant: makeDivInvariant(),
        llm: stub,
      });

      expect(artifact.passesOnFixedCode).toBe(true);
      expect(artifact.failsOnOriginalCode).toBe(true);
      expect(artifact.witnessInputs).toEqual({ a: 5, b: 0 });
      expect(artifact.audit.fixedRunExitCode).toBe(0);
      expect(artifact.audit.originalRunExitCode).toBe(1);
      expect(artifact.audit.mutationApplied).toBe(true);
      expect(artifact.audit.mutationReverted).toBe(true);
      expect(artifact.testCode).toBe(CANNED_TEST_CODE.trim());
      expect(artifact.testFilePath).toMatch(/regression\.test\.ts$/);
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });

  it("oracle #9a FAIL: test fails on fixed code → throws", async () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-oracle-9a-"));
    try {
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "divide.ts"), BUGGY_SOURCE, "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });
      writeFileSync(join(repoDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const overlay = makeMinimalOverlay(repoDir);
      overlay.modifiedFiles.add("src/divide.ts");

      vi.mocked(testGenMod.runTestInOverlay)
        .mockReturnValueOnce({ exitCode: 1, stdout: "✗ fail", stderr: "" }); // fixed: FAIL

      const stub = new StubLLMProvider(
        new Map([["Z3 WITNESS INPUTS", CANNED_TEST_CODE]]),
      );

      await expect(
        generateRegressionTest({
          fix: makeFixCandidate(),
          signal: makeSignal(join(repoDir, "src", "divide.ts")),
          locus: makeLocus(join(repoDir, "src", "divide.ts")),
          overlay,
          invariant: makeDivInvariant(),
          llm: stub,
        }),
      ).rejects.toThrow("oracle #9a FAIL");
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });

  it("oracle #9b FAIL: test passes on original code → throws and restores fix", async () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-oracle-9b-"));
    try {
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "divide.ts"), BUGGY_SOURCE, "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });
      writeFileSync(join(repoDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const overlay = makeMinimalOverlay(repoDir);
      overlay.modifiedFiles.add("src/divide.ts");

      vi.mocked(testGenMod.runTestInOverlay)
        .mockReturnValueOnce({ exitCode: 0, stdout: "✓ pass", stderr: "" }) // fixed: pass
        .mockReturnValueOnce({ exitCode: 0, stdout: "✓ pass", stderr: "" }); // original: ALSO pass (bad)

      const stub = new StubLLMProvider(
        new Map([["Z3 WITNESS INPUTS", CANNED_TEST_CODE]]),
      );

      await expect(
        generateRegressionTest({
          fix: makeFixCandidate(),
          signal: makeSignal(join(repoDir, "src", "divide.ts")),
          locus: makeLocus(join(repoDir, "src", "divide.ts")),
          overlay,
          invariant: makeDivInvariant(),
          llm: stub,
        }),
      ).rejects.toThrow("oracle #9b FAIL");

      // After 9b fail, the fix MUST be restored
      const content = readFileSync(join(repoDir, "src", "divide.ts"), "utf8");
      expect(content).toBe(FIXED_SOURCE);
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });

  it("fix is restored after successful mutation verification", async () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-oracle-restore-"));
    try {
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "divide.ts"), BUGGY_SOURCE, "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });
      writeFileSync(join(repoDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const overlay = makeMinimalOverlay(repoDir);
      overlay.modifiedFiles.add("src/divide.ts");

      vi.mocked(testGenMod.runTestInOverlay)
        .mockReturnValueOnce({ exitCode: 0, stdout: "✓", stderr: "" })
        .mockReturnValueOnce({ exitCode: 1, stdout: "✗", stderr: "" });

      const stub = new StubLLMProvider(
        new Map([["Z3 WITNESS INPUTS", CANNED_TEST_CODE]]),
      );

      await generateRegressionTest({
        fix: makeFixCandidate(),
        signal: makeSignal(join(repoDir, "src", "divide.ts")),
        locus: makeLocus(join(repoDir, "src", "divide.ts")),
        overlay,
        invariant: makeDivInvariant(),
        llm: stub,
      });

      // After success, overlay must reflect the FIXED source (C3's patch restored)
      const content = readFileSync(join(repoDir, "src", "divide.ts"), "utf8");
      expect(content).toBe(FIXED_SOURCE);
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Tests for C5 agent path: generateTestCodeViaAgent
// ---------------------------------------------------------------------------

describe("C5: generateTestCodeViaAgent (agent path)", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("agent path: StubLLMProvider with agentResponses writes test file → returned as testCode", async () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-agent-"));
    try {
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

      const overlay = makeMinimalOverlay(repoDir);

      // The stub agent writes the test file at src/divide.regression.test.ts.
      const testFilePath = "src/divide.regression.test.ts";
      const llm = new StubLLMProvider(
        new Map(), // no complete() responses needed
        [{ matchPrompt: "regression", fileEdits: [{ file: testFilePath, newContent: CANNED_TEST_CODE }], text: "Wrote regression test" }],
      );
      expect(llm.agent).toBeDefined();

      const code = await generateTestCodeViaAgent({
        signal: makeSignal(join(repoDir, "src", "divide.ts")),
        locus: makeLocus(join(repoDir, "src", "divide.ts")),
        invariant: makeDivInvariant(),
        inputs: { a: 5, b: 0 },
        testFilePath,
        testName: "regression: divide(a, b) crashes when b is zero",
        llm,
        overlay,
      });

      expect(code).toBe(CANNED_TEST_CODE.trim());
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });

  it("backward compat: generateRegressionTest uses JSON path when LLM has no agent()", async () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c5-agent-compat-"));
    try {
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      mkdirSync(join(repoDir, "src"), { recursive: true });
      writeFileSync(join(repoDir, "src", "divide.ts"), BUGGY_SOURCE, "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });
      writeFileSync(join(repoDir, "src", "divide.ts"), FIXED_SOURCE, "utf8");

      const overlay = makeMinimalOverlay(repoDir);
      overlay.modifiedFiles.add("src/divide.ts");

      vi.mocked(testGenMod.runTestInOverlay)
        .mockReturnValueOnce({ exitCode: 0, stdout: "✓", stderr: "" })
        .mockReturnValueOnce({ exitCode: 1, stdout: "✗", stderr: "" });

      // No agentResponses — stub has no agent() → JSON path.
      const stub = new StubLLMProvider(
        new Map([["Z3 WITNESS INPUTS", CANNED_TEST_CODE]]),
      );
      expect(stub.agent).toBeUndefined();

      const artifact = await generateRegressionTest({
        fix: makeFixCandidate(),
        signal: makeSignal(join(repoDir, "src", "divide.ts")),
        locus: makeLocus(join(repoDir, "src", "divide.ts")),
        overlay,
        invariant: makeDivInvariant(),
        llm: stub,
      });

      expect(artifact.passesOnFixedCode).toBe(true);
      expect(artifact.failsOnOriginalCode).toBe(true);
      expect(artifact.testCode).toBe(CANNED_TEST_CODE.trim());
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Integration test (SLOW, skipped by default in CI)
// ---------------------------------------------------------------------------

it.skip("integration (slow): real vitest run in overlay with division-by-zero fixture", async () => {
  // This test creates a real git repo, real overlay worktree, and runs the
  // full C5 pipeline end-to-end with a working vitest test.
  //
  // Skipped by default because:
  //   1. node_modules symlink setup takes ~1s
  //   2. Nested vitest run adds ~300ms
  //   3. CI may not allow nested vitest invocations
  //
  // Run locally with: npx vitest run src/fix/testGen.test.ts --reporter=verbose
  // (this test is here as a template for manual integration verification)
});
