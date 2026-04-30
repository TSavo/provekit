import { describe, it, expect } from "vitest";
import { mkdtempSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { makeManageGitHookAction } from "./manageGitHook.js";

class StubInstaller {
  state: "absent" | "present" = "absent";
  install() {
    if (this.state === "present") {
      return { installed: true, path: "/tmp/p/.git/hooks/pre-commit", message: "Hook already installed" };
    }
    this.state = "present";
    return { installed: true, path: "/tmp/p/.git/hooks/pre-commit", message: "Hook installed" };
  }
  uninstall() {
    if (this.state === "absent") {
      return { removed: false, message: "No pre-commit hook found" };
    }
    this.state = "absent";
    return { removed: true, message: "Hook removed" };
  }
  isInstalled() {
    return this.state === "present";
  }
}

describe("manageGitHook action", () => {
  it("delegates install to the installer and surfaces path + message", async () => {
    const stub = new StubInstaller();
    const action = makeManageGitHookAction({
      installerFactory: () => stub as unknown as InstanceType<
        typeof import("../../git/HookInstaller.js").HookInstaller
      >,
    });
    const r = await action.run({ operation: "install", projectRoot: "/tmp/p" });
    expect(r.operation).toBe("install");
    expect(r.installed).toBe(true);
    expect(r.path).toBe("/tmp/p/.git/hooks/pre-commit");
    expect(r.message).toBe("Hook installed");
  });

  it("delegates uninstall and status", async () => {
    const stub = new StubInstaller();
    stub.state = "present";
    const action = makeManageGitHookAction({
      installerFactory: () => stub as unknown as InstanceType<
        typeof import("../../git/HookInstaller.js").HookInstaller
      >,
    });
    const status = await action.run({ operation: "status", projectRoot: "/tmp/p" });
    expect(status.installed).toBe(true);
    const removed = await action.run({ operation: "uninstall", projectRoot: "/tmp/p" });
    expect(removed.removed).toBe(true);
    const status2 = await action.run({ operation: "status", projectRoot: "/tmp/p" });
    expect(status2.installed).toBe(false);
  });

  it("describeResource summarizes the resource for audit", () => {
    const stub = new StubInstaller();
    const action = makeManageGitHookAction({
      installerFactory: () => stub as unknown as InstanceType<
        typeof import("../../git/HookInstaller.js").HookInstaller
      >,
    });
    const desc = action.describeResource({
      operation: "install",
      installed: true,
      path: "/tmp/p/.git/hooks/pre-commit",
      message: "Hook installed",
    });
    expect(desc).toContain("operation=install");
    expect(desc).toContain("installed=true");
    expect(desc).toContain("path=/tmp/p/.git/hooks/pre-commit");
  });

  it("survives in non-git tmp dirs by surfacing the installer's not-a-git-repo message", async () => {
    const tmp = mkdtempSync(join(tmpdir(), "hook-action-test-"));
    const action = makeManageGitHookAction();
    const r = await action.run({ operation: "install", projectRoot: tmp });
    expect(r.message).toMatch(/Not a git repository/);
    expect(r.installed).toBe(false);
  });
});
