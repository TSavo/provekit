/**
 * Hook workflow — registry assembly + manifest loading.
 *
 * Wires plan-hook-operation Stage and manage-git-hook Action into a
 * ProducerRegistry/ActionRegistry pair the workflow runner can drive.
 *
 * The on-disk manifest is at `src/workflows/hook.workflow.yaml`.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import {
  InMemoryActionRegistry,
  InMemoryRegistry,
  type ActionRegistry,
  type ProducerRegistry,
} from "../workflow/registry.js";
import {
  parseManifest,
  type WorkflowManifest,
} from "../workflow/manifest.js";
import {
  PLAN_HOOK_OPERATION_CAPABILITY,
  makePlanHookOperationStage,
  type HookOperation,
} from "../workflow/producers/planHookOperation.js";
import {
  MANAGE_GIT_HOOK_CAPABILITY,
  makeManageGitHookAction,
} from "../workflow/producers/manageGitHook.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "hook.workflow.yaml");

export const HOOK_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const HOOK_STAGE_CAPABILITIES = [
  PLAN_HOOK_OPERATION_CAPABILITY,
] as const;

export const HOOK_ACTION_CAPABILITIES = [
  MANAGE_GIT_HOOK_CAPABILITY,
] as const;

export const HOOK_CAPABILITIES = [
  ...HOOK_STAGE_CAPABILITIES,
  ...HOOK_ACTION_CAPABILITIES,
] as const;

export interface HookRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerHookRegistries(): HookRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(PLAN_HOOK_OPERATION_CAPABILITY, makePlanHookOperationStage());
  actionRegistry.register(
    MANAGE_GIT_HOOK_CAPABILITY,
    makeManageGitHookAction(),
  );

  return { registry, actionRegistry };
}

export function loadHookManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/**
 * Workflow input shape that runManifest expects, encoded for $input refs.
 */
export interface HookWorkflowInput {
  operation: HookOperation;
  projectRoot: string;
}
