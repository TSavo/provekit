/**
 * Cluster-by-shape stage — principalize workflow's structural grouping (P2).
 *
 * Given a corpus of StoredInvariants, group them by structural fingerprint
 * so the workflow can identify the recurring shape before lifting it into
 * a candidate principle. The fingerprint is deliberately coarse for v1:
 *
 *   (smt.kind, sortedBindingSorts, declarationCount)
 *
 * Two invariants with the same kind ("arithmetic"), the same set of
 * SMT sorts on their bindings (e.g. ["Int", "Int"]), and the same count
 * of declared constants share a fingerprint. This isolates "the same
 * mechanism in different files" — which is exactly the latent-pattern
 * detection the principalize spec requires before adversarial validation.
 *
 * Pure: no LLM, no fs. The fingerprint algorithm is the v1 implementation
 * of the structural-clustering primitive; future producers can replace
 * the fingerprint with a richer SMT-AST-aware version without changing
 * the Stage contract.
 *
 * Quiet part: the fingerprint is intentionally lossy. Two invariants
 * that fingerprint together may still differ semantically; the next
 * stage (validate-adversarial) catches the false positives. This Stage
 * is a CHEAP filter, not the final verdict on shape equivalence.
 */

import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";
import type { Stage } from "../types.js";

export const CLUSTER_BY_SHAPE_CAPABILITY = "cluster-by-shape";

export interface ClusterByShapeStageInput {
  invariants: StoredInvariant[];
}

export interface ShapeCluster {
  /**
   * Structural fingerprint — `${kind}|${sortedSorts}|${declCount}`.
   * Stable across runs by construction. Two clusters with the same
   * fingerprint are by definition the same cluster.
   */
  fingerprint: string;
  /** invariant.id values ("propertyHashes") in this cluster. */
  members: string[];
  /** Same data the fingerprint encodes, surfaced for downstream consumers. */
  shape: {
    kind: StoredInvariant["smt"]["kind"];
    bindingSorts: string[];
    declarationCount: number;
  };
}

export interface ClusterByShapeResult {
  /** Clusters sorted by member count descending; ties by fingerprint. */
  clusters: ShapeCluster[];
  /**
   * The most-populous cluster (== clusters[0]) surfaced as a top-level
   * field so the manifest reference language can pass it as a single
   * value to downstream stages without array indexing. Null when the
   * input corpus was empty.
   */
  topCluster: ShapeCluster | null;
  /** Total invariants the input carried — for sanity checks. */
  inputCount: number;
}

export interface MakeClusterByShapeStageDeps {
  producerVersion?: string;
}

export function makeClusterByShapeStage(
  deps: MakeClusterByShapeStageDeps = {},
): Stage<ClusterByShapeStageInput, ClusterByShapeResult> {
  const producedBy = deps.producerVersion ?? "cluster-by-shape@v1";

  return {
    name: "cluster-by-shape",
    producedBy,

    serializeInput(input) {
      return {
        invariantIds: [...input.invariants.map((i) => i.id)].sort(),
        invariantBodies: input.invariants
          .map((i) => ({
            id: i.id,
            kind: i.smt.kind,
            sorts: extractSortedSorts(i),
            decls: i.smt.declarations.length,
          }))
          .sort((a, b) => a.id.localeCompare(b.id)),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ClusterByShapeResult;
    },

    async run(input) {
      const groups = new Map<string, ShapeCluster>();
      for (const inv of input.invariants) {
        const sorts = extractSortedSorts(inv);
        const declCount = inv.smt.declarations.length;
        const fingerprint = `${inv.smt.kind}|${sorts.join(",")}|${declCount}`;
        const cluster = groups.get(fingerprint);
        if (cluster) {
          cluster.members.push(inv.id);
        } else {
          groups.set(fingerprint, {
            fingerprint,
            members: [inv.id],
            shape: {
              kind: inv.smt.kind,
              bindingSorts: sorts,
              declarationCount: declCount,
            },
          });
        }
      }
      const clusters = [...groups.values()].sort((a, b) => {
        if (b.members.length !== a.members.length) {
          return b.members.length - a.members.length;
        }
        return a.fingerprint.localeCompare(b.fingerprint);
      });
      return {
        clusters,
        topCluster: clusters[0] ?? null,
        inputCount: input.invariants.length,
      };
    },
  };
}

/**
 * Sort the SMT sorts of an invariant's local bindings. Graph bindings
 * are skipped — they don't carry an SMT sort, only a relation. Sorted
 * for deterministic fingerprinting regardless of binding order on
 * disk.
 */
function extractSortedSorts(inv: StoredInvariant): string[] {
  const sorts: string[] = [];
  for (const b of inv.bindings) {
    if (b.type === "local") {
      sorts.push(b.sort);
    }
  }
  return sorts.sort();
}
