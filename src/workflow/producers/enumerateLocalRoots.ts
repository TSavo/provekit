/**
 * enumerate-local-roots Stage — list external CIDs the proofkit references
 * but did not mint locally.
 *
 * Spec: docs/specs/2026-04-29-correctness-is-a-hash.md
 *       §"Naming discipline: leaves AND roots, not walks"
 *
 * The set difference IS the framework's completeness primitive: walk every
 * local memento; collect the union of its inputCids; subtract the set of
 * local CIDs. What remains is every external claim this proofkit composed
 * against without locally verifying. Each is a pointer to where audit work
 * needs to happen — the auditor takes the list and traverses externally
 * with their own tooling. The framework does NOT walk.
 *
 * Pure read; cacheable on an empty input. The propertyHash is constant
 * across runs against the same local DB; cache invalidation across DB
 * mutations is the consumer's policy, same as other local-state Stages.
 */

import type { Stage } from "../types.js";
import type { Db } from "../../db/index.js";
import { listAllMementos } from "../../fix/runtime/mementoStore.js";

export const ENUMERATE_LOCAL_ROOTS_CAPABILITY = "enumerate-local-roots";

export type EnumerateLocalRootsStageInput = Record<string, never>;

export interface EnumerateLocalRootsOutput {
  /** External CIDs (referenced via inputCids but not locally minted), sorted. */
  roots: string[];
}

export interface MakeEnumerateLocalRootsStageDeps {
  db: Db;
  producerVersion?: string;
}

export function makeEnumerateLocalRootsStage(
  deps: MakeEnumerateLocalRootsStageDeps,
): Stage<EnumerateLocalRootsStageInput, EnumerateLocalRootsOutput> {
  const producedBy = deps.producerVersion ?? "enumerate-local-roots@v1";

  return {
    name: "enumerate-local-roots",
    producedBy,

    serializeInput() {
      return {};
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as EnumerateLocalRootsOutput;
    },

    async run() {
      const all = listAllMementos(deps.db);
      const localCids = new Set<string>();
      for (const m of all) {
        if (m.cid) localCids.add(m.cid);
      }
      const referenced = new Set<string>();
      for (const m of all) {
        for (const ic of m.inputCids ?? []) {
          if (!localCids.has(ic)) referenced.add(ic);
        }
      }
      const roots = [...referenced].sort();
      return { roots };
    },
  };
}
