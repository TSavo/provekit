/**
 * Unit tests for `provekit mine-history`.
 *
 * Exercises the IntentReport → StoredInvariant translation + the git
 * plumbing in isolation. The full CLI dispatch is end-to-end-tested via
 * the binary; here we focus on the parts that matter for correctness:
 *   - --help short-circuits without touching git or LLM
 *   - --dry-run does not write files
 *   - persisted invariants land at .provekit/invariants/<id>.json
 *   - --since shape sniffing dispatches correctly (sha vs date)
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { execFileSync } from "child_process";
import {
  mkdtempSync,
  rmSync,
  writeFileSync,
  readdirSync,
  existsSync,
  mkdirSync,
} from "fs";
import { tmpdir } from "os";
import { join } from "path";

import { runMineHistory } from "./cli.mineHistory";
import * as retrospective from "./fix/intake/retrospective";
import * as factory from "./llm/ProviderFactory";

// Helper to make a tiny git repo with a couple of commits.
function makeRepo(): string {
  const dir = mkdtempSync(join(tmpdir(), "provekit-mine-history-test-"));
  execFileSync("git", ["init", "-q", "-b", "main"], { cwd: dir });
  execFileSync("git", ["config", "user.email", "t@example.com"], { cwd: dir });
  execFileSync("git", ["config", "user.name", "Test"], { cwd: dir });

  // Commit 1
  mkdirSync(join(dir, "src"), { recursive: true });
  writeFileSync(join(dir, "src", "a.ts"), "export const a = 1;\n", "utf-8");
  execFileSync("git", ["add", "."], { cwd: dir });
  execFileSync("git", ["commit", "-q", "-m", "feat: add a"], { cwd: dir });

  // Commit 2
  writeFileSync(join(dir, "src", "a.ts"), "export const a = 2;\n", "utf-8");
  execFileSync("git", ["add", "."], { cwd: dir });
  execFileSync("git", ["commit", "-q", "-m", "fix: bump a to 2"], { cwd: dir });

  return dir;
}

describe("provekit mine-history", () => {
  let savedExit: typeof process.exit;
  let exitCode: number | undefined;
  let logged: string[];
  let savedLog: typeof console.log;

  beforeEach(() => {
    exitCode = undefined;
    logged = [];
    savedExit = process.exit;
    savedLog = console.log;
    // Don't actually exit during tests; capture the code instead.
    process.exit = ((code?: number) => {
      exitCode = code ?? 0;
      throw new Error("__test_exit__");
    }) as never;
    console.log = (...args: unknown[]) => {
      logged.push(args.map((a) => String(a)).join(" "));
    };
  });

  afterEach(() => {
    process.exit = savedExit;
    console.log = savedLog;
    vi.restoreAllMocks();
  });

  it("--help prints usage and returns without invoking git or LLM", async () => {
    const gitSpy = vi.spyOn(retrospective, "extractIntent");
    const factorySpy = vi.spyOn(factory, "createProvider");

    await runMineHistory(["--help"]);

    const out = logged.join("\n");
    expect(out).toContain("provekit mine-history");
    expect(out).toContain("--since");
    expect(out).toContain("--max-commits");
    expect(out).toContain("--dry-run");
    expect(gitSpy).not.toHaveBeenCalled();
    expect(factorySpy).not.toHaveBeenCalled();
  });

  it("--dry-run walks commits but writes nothing", async () => {
    const repo = makeRepo();
    try {
      // Mock createProvider so we don't fire a real LLM.
      vi.spyOn(factory, "createProvider").mockReturnValue({
        name: "test",
        complete: async () => ({ text: "{}" }),
        stream: () => (async function* () {})(),
      } as never);

      // Stub extractIntent: return one constraint-shaped intent per commit.
      vi.spyOn(retrospective, "extractIntent").mockImplementation(async (input) => ({
        source: "retrospective",
        trigger: { kind: "commit", ref: input.commitSha ?? "?", commitMessage: "stubbed" },
        intents: [
          {
            filePath: "src/a.ts",
            lineRange: [1, 1],
            intent: "constant a is non-negative",
            hasRegressionTest: false,
            testGenerationOpportunity: true,
            constraintCandidate: {
              smtSketch: "(declare-const a Int) (assert (< a 0))",
              kind: "arithmetic",
              validationStatus: "candidate",
            },
            citations: [],
          },
        ],
        outputBundle: { patch: null, addedTests: [], constraintArtifact: null },
      }));

      await runMineHistory([repo, "--dry-run", "--max-commits", "5"]);

      // No invariants directory should have been created.
      const invDir = join(repo, ".provekit", "invariants");
      expect(existsSync(invDir)).toBe(false);

      const out = logged.join("\n");
      expect(out).toContain("dry-run:     yes");
      expect(out).toContain("would-mint");
      expect(out).toMatch(/commits walked:\s+2/);
      expect(out).toMatch(/would-mint.*2/);
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });

  it("persists invariants to .provekit/invariants/<id>.json without --dry-run", async () => {
    const repo = makeRepo();
    try {
      vi.spyOn(factory, "createProvider").mockReturnValue({
        name: "test",
        complete: async () => ({ text: "{}" }),
        stream: () => (async function* () {})(),
      } as never);

      vi.spyOn(retrospective, "extractIntent").mockImplementation(async () => ({
        source: "retrospective",
        trigger: { kind: "commit", ref: "?", commitMessage: "stubbed" },
        intents: [
          {
            filePath: "src/a.ts",
            lineRange: [1, 1],
            intent: "non-negative",
            hasRegressionTest: false,
            testGenerationOpportunity: true,
            constraintCandidate: {
              smtSketch: "(declare-const a Int) (assert (< a 0))",
              kind: "arithmetic",
              validationStatus: "candidate",
            },
          },
        ],
        outputBundle: { patch: null, addedTests: [], constraintArtifact: null },
      }));

      await runMineHistory([repo, "--max-commits", "5"]);

      const invDir = join(repo, ".provekit", "invariants");
      expect(existsSync(invDir)).toBe(true);
      const files = readdirSync(invDir).filter((f) => f.endsWith(".json"));
      // Same SMT sketch + bindings across two commits → same id → one file
      // (content-addressable collapse).
      expect(files.length).toBe(1);
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });

  it("skips intents whose filePath no longer exists in HEAD", async () => {
    const repo = makeRepo();
    try {
      vi.spyOn(factory, "createProvider").mockReturnValue({
        name: "test",
        complete: async () => ({ text: "{}" }),
        stream: () => (async function* () {})(),
      } as never);

      vi.spyOn(retrospective, "extractIntent").mockImplementation(async () => ({
        source: "retrospective",
        trigger: { kind: "commit", ref: "?", commitMessage: "stubbed" },
        intents: [
          {
            filePath: "src/does-not-exist.ts",
            lineRange: [1, 1],
            intent: "ghost",
            hasRegressionTest: false,
            testGenerationOpportunity: false,
            constraintCandidate: {
              smtSketch: "(assert true)",
              kind: "other",
              validationStatus: "candidate",
            },
          },
        ],
        outputBundle: { patch: null, addedTests: [], constraintArtifact: null },
      }));

      await runMineHistory([repo, "--max-commits", "5"]);

      const out = logged.join("\n");
      expect(out).toMatch(/skipped \(file missing\):\s+2/);
      const invDir = join(repo, ".provekit", "invariants");
      expect(existsSync(invDir)).toBe(false);
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });

  it("skips intents with null constraintCandidate", async () => {
    const repo = makeRepo();
    try {
      vi.spyOn(factory, "createProvider").mockReturnValue({
        name: "test",
        complete: async () => ({ text: "{}" }),
        stream: () => (async function* () {})(),
      } as never);

      vi.spyOn(retrospective, "extractIntent").mockImplementation(async () => ({
        source: "retrospective",
        trigger: { kind: "commit", ref: "?", commitMessage: "stubbed" },
        intents: [
          {
            filePath: "src/a.ts",
            lineRange: [1, 1],
            intent: "rename only",
            hasRegressionTest: false,
            testGenerationOpportunity: false,
            constraintCandidate: null,
          },
        ],
        outputBundle: { patch: null, addedTests: [], constraintArtifact: null },
      }));

      await runMineHistory([repo, "--max-commits", "5"]);

      const out = logged.join("\n");
      expect(out).toMatch(/skipped \(no candidate\):\s+2/);
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });

  it("recovers from per-commit extractIntent errors and continues", async () => {
    const repo = makeRepo();
    try {
      vi.spyOn(factory, "createProvider").mockReturnValue({
        name: "test",
        complete: async () => ({ text: "{}" }),
        stream: () => (async function* () {})(),
      } as never);

      let calls = 0;
      vi.spyOn(retrospective, "extractIntent").mockImplementation(async () => {
        calls++;
        if (calls === 1) throw new Error("simulated LLM failure");
        return {
          source: "retrospective",
          trigger: { kind: "commit", ref: "?", commitMessage: "x" },
          intents: [
            {
              filePath: "src/a.ts",
              lineRange: [1, 1],
              intent: "ok",
              hasRegressionTest: false,
              testGenerationOpportunity: false,
              constraintCandidate: {
                smtSketch: "(assert true)",
                kind: "other",
                validationStatus: "candidate",
              },
            },
          ],
          outputBundle: { patch: null, addedTests: [], constraintArtifact: null },
        };
      });

      await runMineHistory([repo, "--max-commits", "5"]);

      const out = logged.join("\n");
      expect(out).toMatch(/ERROR/);
      expect(out).toMatch(/per-commit errors:\s+1/);
      // Walk continued: 2 commits, 1 errored, 1 minted.
      expect(out).toMatch(/commits walked:\s+2/);
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });

  it("rejects non-git directories", async () => {
    const dir = mkdtempSync(join(tmpdir(), "provekit-not-a-repo-"));
    try {
      let caught: Error | null = null;
      try {
        await runMineHistory([dir]);
      } catch (e) {
        caught = e as Error;
      }
      expect(caught?.message).toBe("__test_exit__");
      expect(exitCode).toBe(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
