/**
 * Tests for extractBugs.ts — the BugsJS Phase 1 extractor.
 *
 * Build a synthetic BugsJS-shaped repo on disk per test (Bug-N, Bug-N-fix,
 * Bug-N-test, Bug-N-original) so we exercise the real git plumbing without
 * cloning a multi-megabyte BugsJS fork.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { extractBugs, listBugIds } from "./extractBugs.js";

// ---------------------------------------------------------------------------
// Helpers: build a minimal BugsJS-shaped fixture repo
// ---------------------------------------------------------------------------

const GIT_ID = ["-c", "user.email=t@e.x", "-c", "user.name=T", "-c", "init.defaultBranch=main"];

function gitInit(dir: string): void {
  execFileSync("git", [...GIT_ID, "init", dir], { stdio: "pipe" });
}

function git(dir: string, args: string[]): void {
  execFileSync("git", [...GIT_ID, ...args], { cwd: dir, stdio: "pipe" });
}

function writeFile(dir: string, path: string, content: string): void {
  const abs = join(dir, path);
  mkdirSync(join(abs, ".."), { recursive: true });
  writeFileSync(abs, content, "utf-8");
}

/**
 * Stage a Bug-N tag set inside `repoDir`. Each stage is its own orphan
 * branch + tag so the tags are disjoint commits, mirroring the BugsJS
 * convention. Returns the SHA of Bug-N.
 */
function stageBug(repoDir: string, opts: {
  bugId: string;
  buggy: Record<string, string>;
  fix: Record<string, string>;            // file content at Bug-N-fix
  test?: Record<string, string>;          // file content at Bug-N-test
  upstreamMessage?: string;               // for Bug-N-original
}): void {
  const { bugId, buggy, fix, test, upstreamMessage } = opts;

  // Bug-N — the buggy state.
  git(repoDir, ["checkout", "--orphan", `bug-${bugId}-base`]);
  // Clean working tree
  // Clear staged index without erroring on an empty initial tree.
  execFileSync("git", ["read-tree", "--empty"], { cwd: repoDir, stdio: "pipe" });
  for (const [path, content] of Object.entries(buggy)) {
    writeFile(repoDir, path, content);
    git(repoDir, ["add", path]);
  }
  git(repoDir, ["commit", "-m", `Bug-${bugId}`]);
  git(repoDir, ["tag", `Bug-${bugId}`]);

  // Bug-N-fix — fix applied, no test.
  git(repoDir, ["checkout", "--orphan", `bug-${bugId}-fix`]);
  // Clear staged index without erroring on an empty initial tree.
  execFileSync("git", ["read-tree", "--empty"], { cwd: repoDir, stdio: "pipe" });
  for (const [path, content] of Object.entries({ ...buggy, ...fix })) {
    writeFile(repoDir, path, content);
    git(repoDir, ["add", path]);
  }
  git(repoDir, ["commit", "-m", `Bug-${bugId} fix`]);
  git(repoDir, ["tag", `Bug-${bugId}-fix`]);

  // Bug-N-test — test added, no fix.
  if (test) {
    git(repoDir, ["checkout", "--orphan", `bug-${bugId}-test`]);
    // Clear staged index without erroring on an empty initial tree.
  execFileSync("git", ["read-tree", "--empty"], { cwd: repoDir, stdio: "pipe" });
    for (const [path, content] of Object.entries({ ...buggy, ...test })) {
      writeFile(repoDir, path, content);
      git(repoDir, ["add", path]);
    }
    git(repoDir, ["commit", "-m", `Bug-${bugId} test`]);
    git(repoDir, ["tag", `Bug-${bugId}-test`]);
  }

  // Bug-N-original — synthetic upstream commit (just to carry the message).
  if (upstreamMessage) {
    git(repoDir, ["checkout", "--orphan", `bug-${bugId}-original`]);
    // Clear staged index without erroring on an empty initial tree.
  execFileSync("git", ["read-tree", "--empty"], { cwd: repoDir, stdio: "pipe" });
    for (const [path, content] of Object.entries({ ...buggy, ...fix })) {
      writeFile(repoDir, path, content);
      git(repoDir, ["add", path]);
    }
    git(repoDir, ["commit", "-m", upstreamMessage]);
    git(repoDir, ["tag", `Bug-${bugId}-original`]);
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("extractBugs — happy path", () => {
  let repoDir: string;

  beforeEach(() => {
    repoDir = mkdtempSync(join(tmpdir(), "extract-bugs-"));
    gitInit(repoDir);
  });

  afterEach(() => {
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("extracts a single clean bug pair", () => {
    stageBug(repoDir, {
      bugId: "1",
      buggy: { "lib/foo.js": "function bug() { return 1 / 0; }\n" },
      fix:   { "lib/foo.js": "function bug() { if (denom === 0) return null; return 1 / denom; }\n" },
      test:  { "test/foo.test.js": "it('foo', () => {});\n" },
      upstreamMessage: "Fix division by zero in foo()",
    });

    const result = extractBugs({ projectPath: repoDir, project: "fixture" });
    expect(result.totalBugIds).toBe(1);
    expect(result.skipped).toEqual([]);
    expect(result.candidates).toHaveLength(1);

    const c = result.candidates[0]!;
    expect(c.source.project).toBe("fixture");
    expect(c.source.bugId).toBe("1");
    expect(c.source.baseSha).toMatch(/^[0-9a-f]{40}$/);
    expect(c.source.fixSha).toMatch(/^[0-9a-f]{40}$/);
    expect(c.source.testSha).toMatch(/^[0-9a-f]{40}$/);
    expect(c.source.originalSha).toMatch(/^[0-9a-f]{40}$/);

    expect(c.buggyFiles["lib/foo.js"]).toContain("return 1 / 0");
    expect(c.fixedFiles["lib/foo.js"]).toContain("if (denom === 0)");
    expect(c.diff).toContain("lib/foo.js");
    expect(c.diff).toContain("if (denom === 0)");
    expect(c.upstreamFixMessage).toContain("Fix division by zero");
    expect(c.testFiles["test/foo.test.js"]).toContain("it('foo'");
    expect(c.stats.filesChanged).toBe(1);
    expect(c.stats.insertions).toBeGreaterThan(0);
  }, 30_000);

  it("listBugIds returns only ids with both Bug-N and Bug-N-fix tags", () => {
    stageBug(repoDir, {
      bugId: "1",
      buggy: { "a.js": "1\n" },
      fix:   { "a.js": "2\n" },
    });
    // Add a Bug-2 base only (no Bug-2-fix). It should NOT appear.
    git(repoDir, ["checkout", "--orphan", "bug-2-base"]);
    // Clear staged index without erroring on an empty initial tree.
  execFileSync("git", ["read-tree", "--empty"], { cwd: repoDir, stdio: "pipe" });
    writeFile(repoDir, "x.js", "x\n");
    git(repoDir, ["add", "x.js"]);
    git(repoDir, ["commit", "-m", "Bug-2"]);
    git(repoDir, ["tag", "Bug-2"]);

    const ids = listBugIds(repoDir);
    expect(ids).toEqual(["1"]);
  }, 30_000);
});

describe("extractBugs — filters", () => {
  let repoDir: string;

  beforeEach(() => {
    repoDir = mkdtempSync(join(tmpdir(), "extract-bugs-"));
    gitInit(repoDir);
  });

  afterEach(() => {
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("skips bugs whose fix touches > maxFiles files", () => {
    stageBug(repoDir, {
      bugId: "1",
      buggy: { "a.js": "1\n", "b.js": "2\n", "c.js": "3\n" },
      fix:   { "a.js": "fixed-a\n", "b.js": "fixed-b\n", "c.js": "fixed-c\n" },
    });

    const r = extractBugs({ projectPath: repoDir, project: "fixture", maxFiles: 2 });
    expect(r.candidates).toHaveLength(0);
    expect(r.skipped).toHaveLength(1);
    expect(r.skipped[0]!.reason).toContain("> maxFiles=2");
  }, 30_000);

  it("skips bugs whose total +/- exceeds maxLoc", () => {
    const big = Array.from({ length: 60 }, (_, i) => `line ${i}`).join("\n") + "\n";
    stageBug(repoDir, {
      bugId: "1",
      buggy: { "a.js": "" },
      fix:   { "a.js": big },
    });
    const r = extractBugs({ projectPath: repoDir, project: "fixture", maxLoc: 10 });
    expect(r.candidates).toHaveLength(0);
    expect(r.skipped[0]!.reason).toContain("> maxLoc=10");
  }, 30_000);

  it("skips bugs whose diff touches only test files", () => {
    stageBug(repoDir, {
      bugId: "1",
      buggy: { "test/foo.js": "old\n" },
      fix:   { "test/foo.js": "new\n" },
    });
    const r = extractBugs({ projectPath: repoDir, project: "fixture" });
    expect(r.candidates).toHaveLength(0);
    expect(r.skipped[0]!.reason).toContain("only test/");
  }, 30_000);

  it("classifies .spec.js and __tests__ as test paths", () => {
    stageBug(repoDir, {
      bugId: "1",
      buggy: { "src/__tests__/foo.js": "1\n", "src/bar.spec.js": "2\n" },
      fix:   { "src/__tests__/foo.js": "1f\n", "src/bar.spec.js": "2f\n" },
    });
    const r = extractBugs({ projectPath: repoDir, project: "fixture" });
    expect(r.candidates).toHaveLength(0);
    expect(r.skipped[0]!.reason).toContain("only test/");
  }, 30_000);
});

describe("extractBugs — onlyBugIds + maxBugs", () => {
  let repoDir: string;

  beforeEach(() => {
    repoDir = mkdtempSync(join(tmpdir(), "extract-bugs-"));
    gitInit(repoDir);
  });

  afterEach(() => {
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("onlyBugIds extracts only the listed ids", () => {
    for (const id of ["1", "2", "3"]) {
      stageBug(repoDir, {
        bugId: id,
        buggy: { "a.js": `// bug ${id}\n` },
        fix:   { "a.js": `// fix ${id}\n` },
      });
    }
    const r = extractBugs({ projectPath: repoDir, project: "fixture", onlyBugIds: ["2"] });
    expect(r.candidates).toHaveLength(1);
    expect(r.candidates[0]!.source.bugId).toBe("2");
  }, 30_000);

  it("maxBugs caps the enumeration", () => {
    for (const id of ["1", "2", "3"]) {
      stageBug(repoDir, {
        bugId: id,
        buggy: { "a.js": `// bug ${id}\n` },
        fix:   { "a.js": `// fix ${id}\n` },
      });
    }
    const r = extractBugs({ projectPath: repoDir, project: "fixture", maxBugs: 2 });
    expect(r.candidates).toHaveLength(2);
    const ids = r.candidates.map((c) => c.source.bugId).sort();
    expect(ids).toEqual(["1", "2"]);
  }, 30_000);
});
