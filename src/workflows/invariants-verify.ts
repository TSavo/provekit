/**
 * Invariants-verify workflow — registry assembly + manifest loading.
 *
 * `provekit invariants verify` runs the standing-invariant gate (Z3,
 * no LLM). Single Stage that wraps verifyAllCached / verifyAll.
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
  VERIFY_INVARIANTS_CAPABILITY,
  makeVerifyInvariantsStage,
} from "../workflow/producers/verifyInvariants.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "invariants-verify.workflow.yaml");

export const INVARIANTS_VERIFY_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const INVARIANTS_VERIFY_STAGE_CAPABILITIES = [
  VERIFY_INVARIANTS_CAPABILITY,
] as const;
export const INVARIANTS_VERIFY_ACTION_CAPABILITIES = [] as const;
export const INVARIANTS_VERIFY_CAPABILITIES = [
  ...INVARIANTS_VERIFY_STAGE_CAPABILITIES,
  ...INVARIANTS_VERIFY_ACTION_CAPABILITIES,
] as const;

export interface InvariantsVerifyDeps {}

export interface InvariantsVerifyRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerInvariantsVerifyRegistries(
  _deps: InvariantsVerifyDeps = {},
): InvariantsVerifyRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();
  registry.register(VERIFY_INVARIANTS_CAPABILITY, makeVerifyInvariantsStage());
  return { registry, actionRegistry };
}

export function loadInvariantsVerifyManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

export interface InvariantsVerifyWorkflowInput {
  projectRoot: string;
  timeoutMs?: number;
  maxPaths?: number;
  adversarial: boolean;
}
