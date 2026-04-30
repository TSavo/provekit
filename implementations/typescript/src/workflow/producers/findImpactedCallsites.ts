/**
 * Find-impacted-callsites stage — migrate workflow's downstream impact
 * scan (M3).
 *
 * Reads the project's `.provekit/invariants/` and reports which stored
 * invariants depend on a Removed or Modified propertyHash from the
 * upstream catalog diff. The reported invariants ARE the "user
 * callsites" the migration plan flags.
 *
 * --- Spec gap (load-bearing) ---------------------------------------------
 *
 * The task spec says: "For each user callsite that depends on a
 * removed/modified propertyHash, surface as a null root." A precise
 * implementation requires per-invariant tracking of which upstream
 * propertyHashes a project invariant composes against — i.e. the
 * project memento's `inputCids` would have to include the kit-bridge
 * memento CIDs.
 *
 * StoredInvariant (the on-disk shape under .provekit/invariants/) does
 * NOT carry that composition link today. It's the v1 schema for the
 * standing-runtime spec, which addresses invariants by content hash
 * but doesn't (yet) record which deeper-layer mementos the project's
 * invariants compose against.
 *
 * v1 heuristic, explicitly documented: a project invariant's `id` IS
 * its propertyHash (per hashInvariant() in invariantStore.ts). When
 * that propertyHash appears in the catalog diff's Removed or Modified
 * lists, the project invariant is reported as impacted. This catches
 * the case of a project that COPIED a kit-built-in invariant verbatim
 * — the propertyHashes collide. It does NOT catch the case of a
 * project invariant that calls a kit primitive whose contract
 * changed; that requires the inputCids machinery. Tracked as a
 * spec-gap output field on the result so the migration plan can
 * surface the limitation.
 *
 * Real composition-walking is a follow-up once the standing runtime's
 * invariant store records inputCids alongside the SMT assertion.
 */

import {
  readInvariants,
  type StoredInvariant,
} from "../../fix/runtime/invariantStore.js";
import type { Stage } from "../types.js";
import type { DiffCatalogsResult } from "./diffCatalogs.js";

export const FIND_IMPACTED_CALLSITES_CAPABILITY = "find-impacted-callsites";

export interface FindImpactedCallsitesStageInput {
  /** Host project root containing `.provekit/invariants/`. */
  projectRoot: string;
  diff: DiffCatalogsResult;
}

export interface ImpactedCallsite {
  /** The project invariant's id (== propertyHash). */
  invariantId: string;
  /** Why this invariant was flagged. */
  reason: "removed" | "modified";
  /**
   * The new propertyHash when reason === "modified". When reason ===
   * "removed", null — the contract is gone, no replacement.
   */
  newPropertyHash: string | null;
  /**
   * Where the patch landed (same field as StoredInvariant.callsite).
   * Used by the migration plan's punch list.
   */
  callsite: StoredInvariant["callsite"];
  /** The originating bug summary, for human-readable plan output. */
  originatingBug: string;
}

export interface FindImpactedCallsitesResult {
  /** Reported impacted callsites; sorted by invariantId for stability. */
  impacted: ImpactedCallsite[];
  /**
   * Total count of invariants scanned. Surfaces sensibly when the
   * directory is empty.
   */
  scanned: number;
  /**
   * Spec-gap signal: documents that this v1 implementation matches by
   * propertyHash collision only, not by composition (inputCids). The
   * migration plan repeats this caveat to the user.
   */
  matchStrategy: "propertyHash-collision-v1";
}

export interface MakeFindImpactedCallsitesStageDeps {
  producerVersion?: string;
}

export function makeFindImpactedCallsitesStage(
  deps: MakeFindImpactedCallsitesStageDeps = {},
): Stage<FindImpactedCallsitesStageInput, FindImpactedCallsitesResult> {
  const producedBy = deps.producerVersion ?? "find-impacted-callsites@v1";

  return {
    name: "find-impacted-callsites",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        removedPropertyHashes: [
          ...input.diff.removed.map((d) => d.propertyHash),
        ].sort(),
        modifiedOldPropertyHashes: [
          ...input.diff.modified.map((m) => m.oldPropertyHash),
        ].sort(),
        modifiedMappings: [...input.diff.modified]
          .map((m) => ({
            oldPropertyHash: m.oldPropertyHash,
            newPropertyHash: m.newPropertyHash,
          }))
          .sort((a, b) => a.oldPropertyHash.localeCompare(b.oldPropertyHash)),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as FindImpactedCallsitesResult;
    },

    async run(input) {
      const removedSet = new Set(
        input.diff.removed.map((d) => d.propertyHash),
      );
      const modifiedMap = new Map(
        input.diff.modified.map((m) => [m.oldPropertyHash, m.newPropertyHash]),
      );

      const invariants = readInvariants(input.projectRoot);
      const impacted: ImpactedCallsite[] = [];
      for (const inv of invariants) {
        if (removedSet.has(inv.id)) {
          impacted.push({
            invariantId: inv.id,
            reason: "removed",
            newPropertyHash: null,
            callsite: inv.callsite,
            originatingBug: inv.originatingBug,
          });
          continue;
        }
        const newHash = modifiedMap.get(inv.id);
        if (newHash !== undefined) {
          impacted.push({
            invariantId: inv.id,
            reason: "modified",
            newPropertyHash: newHash,
            callsite: inv.callsite,
            originatingBug: inv.originatingBug,
          });
        }
      }

      impacted.sort((a, b) => a.invariantId.localeCompare(b.invariantId));

      return {
        impacted,
        scanned: invariants.length,
        matchStrategy: "propertyHash-collision-v1",
      };
    },
  };
}
