/**
 * Invariants-paths workflow — registry assembly + manifest loading.
 *
 * `provekit invariants paths <invariantId>` enumerates dataflow paths
 * from a stored invariant's callsite via the substrate path enumerator.
 * Single Stage; no Actions.
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
  ENUMERATE_INVARIANT_PATHS_CAPABILITY,
  makeEnumerateInvariantPathsStage,
} from "../workflow/producers/enumerateInvariantPaths.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "invariants-paths.workflow.yaml");

export const INVARIANTS_PATHS_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const INVARIANTS_PATHS_STAGE_CAPABILITIES = [
  ENUMERATE_INVARIANT_PATHS_CAPABILITY,
] as const;
export const INVARIANTS_PATHS_ACTION_CAPABILITIES = [] as const;
export const INVARIANTS_PATHS_CAPABILITIES = [
  ...INVARIANTS_PATHS_STAGE_CAPABILITIES,
  ...INVARIANTS_PATHS_ACTION_CAPABILITIES,
] as const;

export interface InvariantsPathsDeps {}

export interface InvariantsPathsRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerInvariantsPathsRegistries(
  _deps: InvariantsPathsDeps = {},
): InvariantsPathsRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();
  registry.register(
    ENUMERATE_INVARIANT_PATHS_CAPABILITY,
    makeEnumerateInvariantPathsStage(),
  );
  return { registry, actionRegistry };
}

export function loadInvariantsPathsManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

export interface InvariantsPathsWorkflowInput {
  projectRoot: string;
  invariantId: string;
  maxPaths: number;
}
