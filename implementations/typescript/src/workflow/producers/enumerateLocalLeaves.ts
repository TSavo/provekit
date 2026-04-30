/**
 * enumerate-local-leaves Stage — list mementos this proofkit minted locally.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md
 *       §"Naming discipline: leaves AND roots, not walks"
 *
 * Leaves are the mementos the local proofkit signed. Downstream consumers
 * compose against these by CID. This Stage is a pure read over the local
 * memento store — it does NOT walk into deeper-layer codebases. Optional
 * filters narrow by evidence variant `kind` and producer identity.
 *
 * Cacheable: input is the filter pair; output is the projection. The
 * propertyHash is bound to the (kind, producedBy) tuple. As with other
 * read Stages over the consumer's local DB, cache invalidation across DB
 * mutations is the consumer's policy — the local DB is part of the local
 * trust posture.
 */

import type { Stage } from "../types.js";
import type { Db } from "../../db/index.js";
import {
  listAllMementos,
  type Memento,
  type Verdict,
} from "../../fix/runtime/mementoStore.js";

export const ENUMERATE_LOCAL_LEAVES_CAPABILITY = "enumerate-local-leaves";

export interface EnumerateLocalLeavesStageInput {
  /** Optional evidence-variant kind filter (e.g. "bridge", "z3-model"). */
  kindFilter?: string | null;
  /** Optional producer-identity filter (e.g. "ts-kit@1.0"). */
  producedByFilter?: string | null;
}

export interface LocalLeaf {
  cid: string;
  bindingHash: string;
  propertyHash: string;
  verdict: Verdict;
  producedBy: string;
  /** The evidence variant kind, or null if the row has no typed envelope. */
  evidenceKind: string | null;
  /** inputCids the leaf composes against (sorted). */
  inputCids: string[];
}

export interface EnumerateLocalLeavesOutput {
  /** Filters applied, echoed back for audit. */
  kindFilter: string | null;
  producedByFilter: string | null;
  /** Leaves matching the filter, sorted by CID for stability. */
  leaves: LocalLeaf[];
}

export interface MakeEnumerateLocalLeavesStageDeps {
  db: Db;
  producerVersion?: string;
}

export function makeEnumerateLocalLeavesStage(
  deps: MakeEnumerateLocalLeavesStageDeps,
): Stage<EnumerateLocalLeavesStageInput, EnumerateLocalLeavesOutput> {
  const producedBy = deps.producerVersion ?? "enumerate-local-leaves@v1";

  return {
    name: "enumerate-local-leaves",
    producedBy,

    serializeInput(input) {
      return {
        kindFilter: input.kindFilter ?? null,
        producedByFilter: input.producedByFilter ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as EnumerateLocalLeavesOutput;
    },

    async run(input) {
      const kindFilter = input.kindFilter ?? null;
      const producedByFilter = input.producedByFilter ?? null;
      const all = listAllMementos(deps.db);
      const filtered = all.filter((m) =>
        passesFilters(m, kindFilter, producedByFilter),
      );
      const leaves: LocalLeaf[] = filtered
        .filter((m) => Boolean(m.cid))
        .map((m) => ({
          cid: m.cid!,
          bindingHash: m.bindingHash,
          propertyHash: m.propertyHash,
          verdict: m.verdict,
          producedBy: m.producedBy,
          evidenceKind: m.evidence?.kind ?? null,
          inputCids: [...(m.inputCids ?? [])].sort(),
        }));
      leaves.sort((a, b) => a.cid.localeCompare(b.cid));
      return {
        kindFilter,
        producedByFilter,
        leaves,
      };
    },
  };
}

function passesFilters(
  memento: Memento,
  kindFilter: string | null,
  producedByFilter: string | null,
): boolean {
  if (kindFilter !== null) {
    const kind = memento.evidence?.kind ?? null;
    if (kind !== kindFilter) return false;
  }
  if (producedByFilter !== null) {
    if (memento.producedBy !== producedByFilter) return false;
  }
  return true;
}
