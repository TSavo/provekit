/**
 * manage-git-hook Action — drive HookInstaller's filesystem mutations.
 *
 * Why an Action: writes/removes a pre-commit hook on disk. Side effects.
 * Not cacheable; running it twice in a row may produce different
 * results (idempotent install yields "already installed" the second
 * time, uninstall fails after a successful first uninstall, etc.).
 *
 * Wraps src/git/HookInstaller.ts. The Stage upstream (plan-hook-
 * operation) is the content-addressable claim about WHAT to do; this
 * Action is the actual doing.
 */

import { HookInstaller } from "../../git/HookInstaller.js";
import type { Action } from "../types.js";
import type { HookOperation } from "./planHookOperation.js";

export const MANAGE_GIT_HOOK_CAPABILITY = "manage-git-hook";

export interface ManageGitHookActionInput {
  operation: HookOperation;
  projectRoot: string;
}

export interface ManageGitHookActionResource {
  operation: HookOperation;
  /** Was the hook installed at end of run? (true/false/undefined for status). */
  installed?: boolean;
  /** Was a uninstall actually performed? */
  removed?: boolean;
  /** Filesystem path of the hook file when known. */
  path?: string;
  /** Human-readable summary mirroring the imperative CLI's output. */
  message: string;
}

export interface MakeManageGitHookActionDeps {
  /** Override producer identity. Default: "manage-git-hook@v1". */
  producerVersion?: string;
  /** Test seam: inject a custom installer factory. */
  installerFactory?: (projectRoot: string) => HookInstaller;
}

export function makeManageGitHookAction(
  deps: MakeManageGitHookActionDeps = {},
): Action<ManageGitHookActionInput, ManageGitHookActionResource> {
  const producedBy = deps.producerVersion ?? "manage-git-hook@v1";
  const factory =
    deps.installerFactory ?? ((projectRoot: string) => new HookInstaller(projectRoot));

  return {
    name: "manage-git-hook",
    producedBy,

    serializeInput(input) {
      return {
        operation: input.operation,
        projectRoot: input.projectRoot,
      };
    },

    describeResource(resource) {
      const parts: string[] = [`operation=${resource.operation}`];
      if (resource.installed !== undefined) parts.push(`installed=${resource.installed}`);
      if (resource.removed !== undefined) parts.push(`removed=${resource.removed}`);
      if (resource.path) parts.push(`path=${resource.path}`);
      return parts.join(" ");
    },

    async run(input) {
      const installer = factory(input.projectRoot);
      switch (input.operation) {
        case "install": {
          const r = installer.install();
          return {
            operation: "install",
            installed: r.installed,
            ...(r.path ? { path: r.path } : {}),
            message: r.message,
          };
        }
        case "uninstall": {
          const r = installer.uninstall();
          return {
            operation: "uninstall",
            removed: r.removed,
            message: r.message,
          };
        }
        case "status": {
          const installed = installer.isInstalled();
          return {
            operation: "status",
            installed,
            message: installed ? "Hook installed" : "Hook not installed",
          };
        }
      }
    },
  };
}
