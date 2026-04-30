/**
 * Override workflow — registry assembly + manifest loading.
 *
 * Companion to refute.ts / bug-fix.ts. Wires the single
 * record-override producer to a ProducerRegistry the workflow runner
 * can drive. No actions — this workflow has no side effects.
 *
 * The on-disk manifest is at `src/workflows/override.workflow.yaml`.
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
  RECORD_OVERRIDE_CAPABILITY,
  makeRecordOverrideStage,
} from "../workflow/producers/recordOverride.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "override.workflow.yaml");

export const OVERRIDE_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const OVERRIDE_STAGE_CAPABILITIES = [
  RECORD_OVERRIDE_CAPABILITY,
] as const;

export const OVERRIDE_ACTION_CAPABILITIES = [] as const;

export const OVERRIDE_CAPABILITIES = [
  ...OVERRIDE_STAGE_CAPABILITIES,
  ...OVERRIDE_ACTION_CAPABILITIES,
] as const;

export interface OverrideRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerOverrideRegistries(): OverrideRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(RECORD_OVERRIDE_CAPABILITY, makeRecordOverrideStage());

  return { registry, actionRegistry };
}

export function loadOverrideManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/**
 * Workflow input shape that runManifest expects, encoded for $input refs.
 */
export interface OverrideWorkflowInput {
  /** The justification for bypassing the gate. */
  reason: string;
}
