import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execFileSync } from "child_process";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { join, resolve } from "path";
import { tmpdir } from "os";
import { DiffAnalyzer } from "./DiffAnalyzer";

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, encoding: "utf-8" }).trim();
}

function initRepo(dir: string): void {
  git(dir, "init", "-q");
  git(dir, "config", "user.email", "test@example.com");
  git(dir, "config", "user.name", "Test");
  git(dir, "commit", "--allow-empty", "-m", "init");
}

describe("DiffAnalyzer", () => {
  let projectRoot: string;

  beforeEach(() => {
    projectRoot = mkdtempSync(join(tmpdir(), "provekit-diff-"));
  });

  afterEach(() => {
    rmSync(projectRoot, { recursive: true, force: true });
  });

  it("isGitRepo returns false outside a repo and true inside", () => {
    expect(new DiffAnalyzer(projectRoot).isGitRepo()).toBe(false);
    initRepo(projectRoot);
    expect(new DiffAnalyzer(projectRoot).isGitRepo()).toBe(true);
  });

  it("getHead returns null outside a repo, sha inside", () => {
    expect(new DiffAnalyzer(projectRoot).getHead()).toBeNull();
    initRepo(projectRoot);
    const head = new DiffAnalyzer(projectRoot).getHead();
    expect(head).toMatch(/^[0-9a-f]{40}$/);
  });

  it("getStagedChanges classifies added/modified/deleted files", () => {
    initRepo(projectRoot);
    writeFileSync(join(projectRoot, "a.ts"), "export const a = 1;\n");
    writeFileSync(join(projectRoot, "b.ts"), "export const b = 1;\n");
    git(projectRoot, "add", "a.ts", "b.ts");
    git(projectRoot, "commit", "-m", "seed");

    writeFileSync(join(projectRoot, "a.ts"), "export const a = 2;\n");
    writeFileSync(join(projectRoot, "c.ts"), "export const c = 3;\n");
    execFileSync("rm", [join(projectRoot, "b.ts")]);
    git(projectRoot, "add", "-A");

    const changes = new DiffAnalyzer(projectRoot).getStagedChanges();
    expect(changes.addedFiles).toContain("c.ts");
    expect(changes.modifiedFiles).toContain("a.ts");
    expect(changes.deletedFiles).toContain("b.ts");
  });

  it("getChangedTypeScriptFiles filters node_modules and .d.ts, returns absolute paths", () => {
    initRepo(projectRoot);
    mkdirSync(join(projectRoot, "node_modules", "pkg"), { recursive: true });
    writeFileSync(join(projectRoot, "src.ts"), "x");
    writeFileSync(join(projectRoot, "types.d.ts"), "declare const x: number;");
    writeFileSync(join(projectRoot, "node_modules", "pkg", "n.ts"), "x");
    git(projectRoot, "add", "-A");

    const tsFiles = new DiffAnalyzer(projectRoot).getChangedTypeScriptFiles();
    const realRoot = resolve(projectRoot);
    expect(tsFiles).toContain(resolve(realRoot, "src.ts"));
    expect(tsFiles.some((f) => f.includes("node_modules"))).toBe(false);
    expect(tsFiles.some((f) => f.endsWith(".d.ts"))).toBe(false);
  });

  it("getStagedChanges returns empty result when not a repo (git fails)", () => {
    const result = new DiffAnalyzer(projectRoot).getStagedChanges();
    expect(result.changedFiles).toEqual([]);
    expect(result.addedFiles).toEqual([]);
  });
});
