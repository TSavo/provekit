/**
 * Diff workflow — registry assembly + manifest loading.
 *
 * Meaning-layer counterpart to `git diff`. Composes four Stages, all of
 * which are LLM-free by design. The one Z3 dependency lives inside the
 * synthesize step's check-implication probes; no other external calls.
 */

import { dirname, join } from "path";
import { readFileSync } from "fs";
import { fileURLToPath } from "url";
import {
  InMemoryRegistry,
  type ProducerRegistry,
} from "../workflow/registry.js";
import { parseManifest, type WorkflowManifest } from "../workflow/manifest.js";
import {
  RESOLVE_INVARIANT_SNAPSHOT_CAPABILITY,
  makeResolveInvariantSnapshotStage,
} from "../workflow/producers/resolveInvariantSnapshot.js";
import {
  DIFF_INVARIANT_SNAPSHOTS_CAPABILITY,
  makeDiffInvariantSnapshotsStage,
} from "../workflow/producers/diffInvariantSnapshots.js";
import {
  SYNTHESIZE_MEANING_DIFF_CAPABILITY,
  makeSynthesizeMeaningDiffStage,
} from "../workflow/producers/synthesizeMeaningDiff.js";

const __dirname = dirname(fileURLToPath(import.meta.url));

export interface DiffWorkflowDeps {
  /** Unused; the diff workflow is LLM-free. Accepted for dispatcher uniformity. */
  llm?: unknown;
}

export interface DiffRegistries {
  registry: ProducerRegistry;
}

export function registerDiffRegistries(_deps: DiffWorkflowDeps): DiffRegistries {
  const registry = new InMemoryRegistry();
  registry.register(RESOLVE_INVARIANT_SNAPSHOT_CAPABILITY, makeResolveInvariantSnapshotStage());
  registry.register(DIFF_INVARIANT_SNAPSHOTS_CAPABILITY, makeDiffInvariantSnapshotsStage());
  registry.register(SYNTHESIZE_MEANING_DIFF_CAPABILITY, makeSynthesizeMeaningDiffStage());
  return { registry };
}

export function loadDiffManifest(): WorkflowManifest {
  const yaml = readFileSync(join(__dirname, "diff.workflow.yaml"), "utf-8");
  return parseManifest(yaml);
}
