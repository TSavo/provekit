/**
 * Explain workflow — registry assembly + manifest loading.
 *
 * The simplest of the four data-driven workflows: a single Stage
 * (`render-proof-chain`) that walks the consumer's local memento store
 * and returns a human-readable rendering. No Actions, no LLMs, no
 * network calls. Reuses the shared `renderProofChain` producer.
 *
 * Scope discipline:
 *   docs/specs/2026-04-29-correctness-is-a-hash.md §"What ProvekIt is"
 *   The framework operates on its OWN local leaves and lists its OWN
 *   roots — it does not traverse external CIDs. This workflow makes
 *   that boundary visible by surfacing `unresolvedInputCids` rather
 *   than chasing them.
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
  RENDER_PROOF_CHAIN_CAPABILITY,
  makeRenderProofChainStage,
} from "../workflow/producers/renderProofChain.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "explain.workflow.yaml");

export const EXPLAIN_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const EXPLAIN_STAGE_CAPABILITIES = ["render-proof-chain"] as const;
export const EXPLAIN_ACTION_CAPABILITIES = [] as const;
export const EXPLAIN_CAPABILITIES = [
  ...EXPLAIN_STAGE_CAPABILITIES,
  ...EXPLAIN_ACTION_CAPABILITIES,
] as const;

export interface ExplainDeps {
  db: Db;
}

export interface ExplainRegistries {
  registry: ProducerRegistry;
}

/**
 * Construct the stage ProducerRegistry for the explain workflow,
 * populated with every capability the on-disk manifest references.
 */
export function registerExplainRegistries(deps: ExplainDeps): ExplainRegistries {
  const registry = new InMemoryRegistry();
  registry.register(
    RENDER_PROOF_CHAIN_CAPABILITY,
    makeRenderProofChainStage({ db: deps.db }),
  );
  return { registry };
}

/**
 * Read and parse the explain manifest from disk.
 */
export function loadExplainManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape that runManifest expects, encoded for $input refs. */
export interface ExplainWorkflowInput {
  /** CID of the memento to render the chain from. */
  startCid: string;
  /** Optional maximum DAG depth to traverse. */
  maxDepth?: number;
}
