/**
 * Retire workflow — registry assembly + manifest loading.
 *
 * Deprecates an existing invariant by minting a `verdict: decayed`
 * memento via the `mint-deprecation` Stage and appending a
 * `must.skip(...)` marker to the on-disk `.invariant.ts` file via
 * the `write-invariant-file` Action.
 *
 * Stage and Action are independent: the Stage is the durable proof
 * (the memento), the Action is the human-readable hint. The manifest
 * orders the Action runAfter the Stage so the audit memento always
 * exists before the file is touched.
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
  MINT_DEPRECATION_CAPABILITY,
  makeMintDeprecationStage,
} from "../workflow/producers/mintDeprecation.js";
import {
  WRITE_INVARIANT_FILE_CAPABILITY,
  makeWriteInvariantFileAction,
} from "../workflow/producers/writeInvariantFile.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "retire.workflow.yaml");

export const RETIRE_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const RETIRE_STAGE_CAPABILITIES = ["mint-deprecation"] as const;
export const RETIRE_ACTION_CAPABILITIES = ["write-invariant-file"] as const;
export const RETIRE_CAPABILITIES = [
  ...RETIRE_STAGE_CAPABILITIES,
  ...RETIRE_ACTION_CAPABILITIES,
] as const;

export interface RetireDeps {
  // Intentionally empty: retire's producers don't need a Db handle
  // (the runner provides one via WorkflowRunner). Kept as a type for
  // shape-parity with the other workflows so callers can pass a deps
  // object uniformly.
}

export interface RetireRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

/** Construct stage + action registries for the retire workflow. */
export function registerRetireRegistries(
  _deps: RetireDeps = {},
): RetireRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(MINT_DEPRECATION_CAPABILITY, makeMintDeprecationStage());
  actionRegistry.register(
    WRITE_INVARIANT_FILE_CAPABILITY,
    makeWriteInvariantFileAction(),
  );

  return { registry, actionRegistry };
}

/** Read and parse the retire manifest from disk. */
export function loadRetireManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape that runManifest expects. */
export interface RetireWorkflowInput {
  /** propertyHash of the invariant being retired. */
  retiredPropertyHash: string;
  /** Property name (matches the property("name", ...) declaration). */
  propertyName: string;
  /** Reason for retiring. Required — no silent retires. */
  reason: string;
  /** Absolute path to the `.invariant.ts` file. */
  filePath: string;
  /**
   * Source text appended to the file as the human-readable
   * deprecation hint. Caller-controlled so workflows can shape the
   * marker (must.skip wrapper, leading newline, comment block) to
   * match local conventions.
   */
  skipMarker: string;
}
