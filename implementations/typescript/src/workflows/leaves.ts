/**
 * Leaves workflow — registry assembly + manifest loading.
 *
 * Two Stages, no Actions: enumerate-local-leaves projects the local
 * memento store; format-leaves-output renders text or JSON. The DB is
 * a runtime dependency, not part of any Stage's input — same pattern as
 * explain / load-catalog.
 *
 * Scope discipline:
 *   protocol/specs/2026-04-29-correctness-is-a-hash.md §"What ProvekIt is"
 *   The framework MINTS local mementos and ENUMERATES local state. The
 *   leaves query is the "what did we contribute?" surface — local, free,
 *   complete.
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
  ENUMERATE_LOCAL_LEAVES_CAPABILITY,
  makeEnumerateLocalLeavesStage,
} from "../workflow/producers/enumerateLocalLeaves.js";
import {
  FORMAT_LEAVES_OUTPUT_CAPABILITY,
  makeFormatLeavesOutputStage,
} from "../workflow/producers/formatLeavesOutput.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "leaves.workflow.yaml");

export const LEAVES_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const LEAVES_STAGE_CAPABILITIES = [
  "enumerate-local-leaves",
  "format-leaves-output",
] as const;
export const LEAVES_ACTION_CAPABILITIES = [] as const;
export const LEAVES_CAPABILITIES = [
  ...LEAVES_STAGE_CAPABILITIES,
  ...LEAVES_ACTION_CAPABILITIES,
] as const;

export interface LeavesDeps {
  db: Db;
}

export interface LeavesRegistries {
  registry: ProducerRegistry;
}

/**
 * Construct the stage ProducerRegistry for the leaves workflow,
 * populated with every capability the on-disk manifest references.
 */
export function registerLeavesRegistries(deps: LeavesDeps): LeavesRegistries {
  const registry = new InMemoryRegistry();
  registry.register(
    ENUMERATE_LOCAL_LEAVES_CAPABILITY,
    makeEnumerateLocalLeavesStage({ db: deps.db }),
  );
  registry.register(
    FORMAT_LEAVES_OUTPUT_CAPABILITY,
    makeFormatLeavesOutputStage(),
  );
  return { registry };
}

/**
 * Read and parse the leaves manifest from disk.
 */
export function loadLeavesManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape that runManifest expects, encoded for $input refs. */
export interface LeavesWorkflowInput {
  /** Output format: "text" (default) or "json". */
  format?: string | null;
  /** Filter by evidence-variant kind, e.g. "bridge". */
  kind?: string | null;
  /** Filter by producer identity, e.g. "ts-kit@1.0". */
  producedBy?: string | null;
}
