import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execFileSync } from "child_process";
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { HookInstaller } from "./HookInstaller";

const HOOK_MARKER = "# provekit pre-commit hook";

function initRepo(dir: string): void {
  execFileSync("git", ["init", "-q"], { cwd: dir });
}

describe("HookInstaller", () => {
  let projectRoot: string;

  beforeEach(() => {
    projectRoot = mkdtempSync(join(tmpdir(), "provekit-hook-"));
  });

  afterEach(() => {
    rmSync(projectRoot, { recursive: true, force: true });
  });

  it("install reports failure when not a git repo", () => {
    const result = new HookInstaller(projectRoot).install();
    expect(result.installed).toBe(false);
    expect(result.message).toMatch(/Not a git repository/);
  });

  it("isInstalled returns false when no hook present", () => {
    initRepo(projectRoot);
    expect(new HookInstaller(projectRoot).isInstalled()).toBe(false);
  });

  it("install creates pre-commit and isInstalled returns true", () => {
    initRepo(projectRoot);
    const installer = new HookInstaller(projectRoot);

    const r = installer.install();
    expect(r.installed).toBe(true);
    expect(r.path).toMatch(/pre-commit$/);
    expect(existsSync(r.path)).toBe(true);
    expect(readFileSync(r.path, "utf-8")).toContain(HOOK_MARKER);
    expect(installer.isInstalled()).toBe(true);
  });

  it("install is idempotent: second call reports already installed", () => {
    initRepo(projectRoot);
    const installer = new HookInstaller(projectRoot);
    installer.install();
    const r2 = installer.install();
    expect(r2.installed).toBe(true);
    expect(r2.message).toMatch(/already installed/);
  });

  it("install appends to an existing non-provekit hook without clobbering it", () => {
    initRepo(projectRoot);
    const hookPath = join(projectRoot, ".git", "hooks", "pre-commit");
    mkdirSync(join(projectRoot, ".git", "hooks"), { recursive: true });
    writeFileSync(hookPath, "#!/bin/sh\necho existing hook\n");

    const installer = new HookInstaller(projectRoot);
    const r = installer.install();
    expect(r.installed).toBe(true);
    expect(r.message).toMatch(/appended/i);
    const contents = readFileSync(hookPath, "utf-8");
    expect(contents).toContain("echo existing hook");
    expect(contents).toContain(HOOK_MARKER);
  });

  it("uninstall reports no hook when nothing present", () => {
    initRepo(projectRoot);
    const r = new HookInstaller(projectRoot).uninstall();
    expect(r.removed).toBe(false);
    expect(r.message).toMatch(/No pre-commit hook/);
  });

  it("uninstall refuses to remove a non-provekit hook", () => {
    initRepo(projectRoot);
    const hookPath = join(projectRoot, ".git", "hooks", "pre-commit");
    mkdirSync(join(projectRoot, ".git", "hooks"), { recursive: true });
    writeFileSync(hookPath, "#!/bin/sh\necho mine\n");

    const r = new HookInstaller(projectRoot).uninstall();
    expect(r.removed).toBe(false);
    expect(r.message).toMatch(/not installed by provekit/i);
  });

  it("install + uninstall removes the file when only provekit hook present", () => {
    initRepo(projectRoot);
    const installer = new HookInstaller(projectRoot);
    const installed = installer.install();
    expect(installed.installed).toBe(true);

    const removed = installer.uninstall();
    expect(removed.removed).toBe(true);
    expect(installer.isInstalled()).toBe(false);
  });
});
