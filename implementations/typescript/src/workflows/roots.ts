/**
 * Roots workflow — registry assembly + manifest loading.
 *
 * Two Stages, no Actions: enumerate-local-roots computes the set
 * difference (referenced CIDs minus locally-minted CIDs); format-roots-
 * output renders text or JSON. The DB is a runtime dependency, not part
 * of any Stage's input — same pattern as explain / leaves.
 *
 * Scope discipline:
 *   protocol/specs/2026-04-29-correctness-is-a-hash.md §"What ProvekIt is"
 *   `provekit roots` surfaces "where audit needs to happen" without
 *   walking external CIDs. The framework hands out the list; auditors
 *   take it and traverse externally with their own tooling.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { Db } from "../db/index.js";
import {
  InMemoryRegistry,
  type ProducerRegistry,
} from "../workflow/registry.js";
import {
  parseManifest,
  type WorkflowManifest,
} from "../workflow/manifest.js";
import {
  ENUMERATE_LOCAL_ROOTS_CAPABILITY,
  makeEnumerateLocalRootsStage,
} from "../workflow/producers/enumerateLocalRoots.js";
import {
  FORMAT_ROOTS_OUTPUT_CAPABILITY,
  makeFormatRootsOutputStage,
} from "../workflow/producers/formatRootsOutput.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "roots.workflow.yaml");

export const ROOTS_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const ROOTS_STAGE_CAPABILITIES = [
  "enumerate-local-roots",
  "format-roots-output",
] as const;
export const ROOTS_ACTION_CAPABILITIES = [] as const;
export const ROOTS_CAPABILITIES = [
  ...ROOTS_STAGE_CAPABILITIES,
  ...ROOTS_ACTION_CAPABILITIES,
] as const;

export interface RootsDeps {
  db: Db;
}

export interface RootsRegistries {
  registry: ProducerRegistry;
}

/**
 * Construct the stage ProducerRegistry for the roots workflow,
 * populated with every capability the on-disk manifest references.
 */
export function registerRootsRegistries(deps: RootsDeps): RootsRegistries {
  const registry = new InMemoryRegistry();
  registry.register(
    ENUMERATE_LOCAL_ROOTS_CAPABILITY,
    makeEnumerateLocalRootsStage({ db: deps.db }),
  );
  registry.register(
    FORMAT_ROOTS_OUTPUT_CAPABILITY,
    makeFormatRootsOutputStage(),
  );
  return { registry };
}

/**
 * Read and parse the roots manifest from disk.
 */
export function loadRootsManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape that runManifest expects, encoded for $input refs. */
export interface RootsWorkflowInput {
  /** Output format: "text" (default) or "json". */
  format?: string | null;
}
