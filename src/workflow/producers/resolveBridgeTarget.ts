/**
 * resolve-bridge-target — Stage 3 of the bridge enforcement workflow.
 *
 * Hash lookup: given a bridge's targetContractCid + the unified
 * memento pool from load-all-proofs, returns the resolved property
 * memento's IrFormula. O(1) — no file IO; the pool was built once at
 * Stage 1.
 *
 * Returns null when the target CID isn't in the pool (target memento
 * not locally available — install the missing kit) or when the matched
 * memento isn't a property variant.
 */

import type { Stage } from "../types.js";
import type { ClaimEnvelope, PropertyEvidence } from "../../claimEnvelope/types.js";
import type { IrFormula, BindingScope } from "../../ir/formulas.js";

export const RESOLVE_BRIDGE_TARGET_CAPABILITY = "resolve-bridge-target";

export interface ResolveBridgeTargetInput {
  bridgeTargetContractCid: string;
  mementoPool: Record<string, ClaimEnvelope>;
}

export interface ResolvedProperty {
  cid: string;
  irFormula: IrFormula;
  scope: BindingScope;
  irKitVersion: string;
}

export interface ResolveBridgeTargetOutput {
  resolved: ResolvedProperty | null;
  failureReason: "not-in-pool" | "not-property-variant" | null;
}

export interface MakeResolveBridgeTargetStageDeps {
  producerVersion?: string;
}

export function makeResolveBridgeTargetStage(
  deps: MakeResolveBridgeTargetStageDeps = {},
): Stage<ResolveBridgeTargetInput, ResolveBridgeTargetOutput> {
  const producedBy = deps.producerVersion ?? "resolveBridgeTarget@v2";

  return {
    name: "resolveBridgeTarget",
    producedBy,

    serializeInput(input) {
      // Cache key: target CID + pool snapshot (sorted member CIDs).
      // The pool is what determines whether a CID resolves; pool changes
      // → cache invalidates.
      return {
        bridgeTargetContractCid: input.bridgeTargetContractCid,
        poolCids: Object.keys(input.mementoPool).sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ResolveBridgeTargetOutput;
    },

    async run(input) {
      const envelope = input.mementoPool[input.bridgeTargetContractCid];
      if (!envelope) {
        return { resolved: null, failureReason: "not-in-pool" };
      }
      if (envelope.evidence?.kind !== "property") {
        return { resolved: null, failureReason: "not-property-variant" };
      }
      const ev = envelope.evidence as PropertyEvidence;
      return {
        resolved: {
          cid: input.bridgeTargetContractCid,
          irFormula: ev.body.irFormula as IrFormula,
          scope: ev.body.scope as BindingScope,
          irKitVersion: ev.body.irKitVersion,
        },
        failureReason: null,
      };
    },
  };
}
