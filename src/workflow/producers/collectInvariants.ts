/**
 * Collect-invariants stage — principalize workflow's corpus gather (P1).
 *
 * Reads the project's `.provekit/invariants/` via readInvariants() and
 * filters down to the propertyHashes the caller named. The
 * propertyHash for a StoredInvariant is its content-addressable `id`
 * (sha256 prefix of smt + bindings, see hashInvariant() in
 * invariantStore.ts) — same value the standing runtime uses to address
 * invariants. The Stage exists so the corpus selection becomes a
 * cacheable, content-addressed step in the principalize pipeline.
 *
 * Pure: no LLM, no Z3, no fs writes. The `.provekit/invariants/` dir
 * is read on every invocation; it is NOT hashed into the input. Same
 * caveat formulate.ts and recognize.ts document — when invariants on
 * disk change, the cached collect-invariants result silently goes
 * stale. Acceptable for v1; the invariant-store-content-hash binding
 * is a follow-up.
 *
 * Output is the array of matching StoredInvariants in the order
 * readInvariants returns them (createdAt ascending, deterministic).
 */

import {
  readInvariants,
  type StoredInvariant,
} from "../../fix/runtime/invariantStore.js";
import type { Stage } from "../types.js";

export const COLLECT_INVARIANTS_CAPABILITY = "collect-invariants";

export interface CollectInvariantsStageInput {
  /** Host project root containing `.provekit/invariants/`. */
  projectRoot: string;
  /**
   * Whitelist of invariant ids (StoredInvariant.id) to include.
   * When omitted, every non-retired invariant in the store is returned.
   * The whitelist is matched against StoredInvariant.id which IS the
   * propertyHash per the runtime spec.
   */
  propertyHashes?: string[];
}

export interface CollectInvariantsResult {
  /** Matching invariants. Empty when nothing matched or store is empty. */
  invariants: StoredInvariant[];
  /** Number of invariants matched (== invariants.length). Convenience. */
  matchCount: number;
}

export interface MakeCollectInvariantsStageDeps {
  /** Override producer identity. Default: "collect-invariants@v1". */
  producerVersion?: string;
}

export function makeCollectInvariantsStage(
  deps: MakeCollectInvariantsStageDeps = {},
): Stage<CollectInvariantsStageInput, CollectInvariantsResult> {
  const producedBy = deps.producerVersion ?? "collect-invariants@v1";

  return {
    name: "collect-invariants",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        propertyHashes: input.propertyHashes
          ? [...input.propertyHashes].sort()
          : null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as CollectInvariantsResult;
    },

    async run(input) {
      const all = readInvariants(input.projectRoot);
      const invariants =
        input.propertyHashes === undefined
          ? all
          : all.filter((inv) => input.propertyHashes!.includes(inv.id));
      return { invariants, matchCount: invariants.length };
    },
  };
}
