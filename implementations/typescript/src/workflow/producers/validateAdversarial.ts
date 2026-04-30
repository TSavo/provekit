/**
 * Validate-adversarial stage — principalize workflow's false-positive check (P3).
 *
 * Per the task's cut list: real cross-codebase adversarial validation is a
 * follow-up. This v1 implementation is the explicit stub the principalize
 * spec calls for: re-run the proposed principle's shape against the LOCAL
 * invariant corpus and confirm it doesn't match invariants that do not
 * belong to the cluster. The Stage exists so the workflow's pipeline shape
 * is correct; the producer ships now and gets replaced by a real
 * cross-corpus adversary later without changing the Stage contract.
 *
 * Algorithm (v1):
 *   1. Read the cluster's fingerprint.
 *   2. Walk the full invariant corpus.
 *   3. For each non-cluster invariant whose fingerprint matches the
 *      cluster's, record it as a falsePositive.
 *   4. Return verdict "clean" when falsePositives is empty,
 *      "false-positive" otherwise.
 *
 * Pure: no LLM, no Z3, no fs. The "did the principle fire on stuff it
 * shouldn't?" check is the SHAPE of adversarial validation; the v1
 * implementation just delegates to the same fingerprint as
 * cluster-by-shape. When a real adversary lands, this Stage's run()
 * body changes; serializeInput/serializeOutput stay identical so old
 * mementos remain meaningful (they'll just have the v1 producedBy tag).
 */

import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";
import type { Stage } from "../types.js";
import type { ShapeCluster } from "./clusterByShape.js";

export const VALIDATE_ADVERSARIAL_CAPABILITY = "validate-adversarial";

export interface ValidateAdversarialStageInput {
  /**
   * The cluster being lifted into a candidate principle. Null when the
   * upstream cluster-by-shape stage saw an empty corpus; in that case
   * the verdict short-circuits to "clean" with an empty falsePositives
   * list and a distinctive validator tag.
   */
  cluster: ShapeCluster | null;
  /**
   * The full invariant corpus to validate against. Typically the same
   * StoredInvariant[] collect-invariants returned. The Stage walks
   * `corpus`, finds entries whose fingerprint matches `cluster`'s but
   * whose id is NOT in `cluster.members`, and reports them.
   */
  corpus: StoredInvariant[];
}

export interface ValidateAdversarialResult {
  verdict: "clean" | "false-positive";
  /** Invariant ids that fingerprint-matched but aren't cluster members. */
  falsePositives: string[];
  /**
   * Tag describing what kind of validator ran. v1 stub returns
   * "local-fingerprint-only" for normal runs and
   * "empty-corpus-short-circuit" when the cluster was null.
   */
  validator: "local-fingerprint-only" | "empty-corpus-short-circuit";
}

export interface MakeValidateAdversarialStageDeps {
  producerVersion?: string;
}

export function makeValidateAdversarialStage(
  deps: MakeValidateAdversarialStageDeps = {},
): Stage<ValidateAdversarialStageInput, ValidateAdversarialResult> {
  const producedBy = deps.producerVersion ?? "validate-adversarial@v1-stub";

  return {
    name: "validate-adversarial",
    producedBy,

    serializeInput(input) {
      return {
        clusterFingerprint: input.cluster?.fingerprint ?? null,
        clusterMembers: input.cluster
          ? [...input.cluster.members].sort()
          : null,
        corpusIds: [...input.corpus.map((i) => i.id)].sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ValidateAdversarialResult;
    },

    async run(input) {
      if (input.cluster === null) {
        return {
          verdict: "clean",
          falsePositives: [],
          validator: "empty-corpus-short-circuit",
        };
      }
      const memberSet = new Set(input.cluster.members);
      const falsePositives: string[] = [];
      for (const inv of input.corpus) {
        if (memberSet.has(inv.id)) continue;
        const fingerprint = invariantFingerprint(inv);
        if (fingerprint === input.cluster.fingerprint) {
          falsePositives.push(inv.id);
        }
      }
      falsePositives.sort();
      return {
        verdict: falsePositives.length === 0 ? "clean" : "false-positive",
        falsePositives,
        validator: "local-fingerprint-only",
      };
    },
  };
}

/**
 * Same fingerprint algorithm as clusterByShape.ts. Duplicated rather
 * than imported so this Stage's verdict semantics stay self-contained
 * — when the adversary becomes a real cross-codebase walker, this
 * function disappears and the cluster's fingerprint is one of many
 * inputs to the new validator.
 */
function invariantFingerprint(inv: StoredInvariant): string {
  const sorts: string[] = [];
  for (const b of inv.bindings) {
    if (b.type === "local") sorts.push(b.sort);
  }
  sorts.sort();
  return `${inv.smt.kind}|${sorts.join(",")}|${inv.smt.declarations.length}`;
}
