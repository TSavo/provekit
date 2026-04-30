/**
 * Invariants-list workflow — registry assembly + manifest loading.
 *
 * `provekit invariants list [projectRoot] [--all]` reads the per-codebase
 * invariant store and reports every entry. Single Stage, no Actions.
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
  LIST_INVARIANTS_CAPABILITY,
  makeListInvariantsStage,
} from "../workflow/producers/listInvariants.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "invariants-list.workflow.yaml");

export const INVARIANTS_LIST_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const INVARIANTS_LIST_STAGE_CAPABILITIES = [
  LIST_INVARIANTS_CAPABILITY,
] as const;
export const INVARIANTS_LIST_ACTION_CAPABILITIES = [] as const;
export const INVARIANTS_LIST_CAPABILITIES = [
  ...INVARIANTS_LIST_STAGE_CAPABILITIES,
  ...INVARIANTS_LIST_ACTION_CAPABILITIES,
] as const;

export interface InvariantsListDeps {}

export interface InvariantsListRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerInvariantsListRegistries(
  _deps: InvariantsListDeps = {},
): InvariantsListRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();
  registry.register(LIST_INVARIANTS_CAPABILITY, makeListInvariantsStage());
  return { registry, actionRegistry };
}

export function loadInvariantsListManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

export interface InvariantsListWorkflowInput {
  projectRoot: string;
  includeRetired: boolean;
}
