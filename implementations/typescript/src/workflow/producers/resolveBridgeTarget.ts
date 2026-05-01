/**
 * resolve-bridge-target — Stage 3 of the bridge enforcement workflow.
 *
 * Hash lookup: given a bridge's targetContractCid + the unified
 * memento pool from load-all-proofs, returns the resolved contract
 * memento's pre-formula (the precondition the verifier instantiates
 * at the call site). O(1) — no file IO; the pool was built once at
 * Stage 1.
 *
 * Returns null when the target CID isn't in the pool (target memento
 * not locally available — install the missing kit), when the matched
 * memento isn't a contract variant, or when the contract has no `pre`
 * formula (preconditions are the bridge-enforcement obligation source).
 */

import type { Stage } from "../types.js";
import type { ClaimEnvelope, ContractEvidence } from "../../claimEnvelope/types.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { MementoPool } from "../../verifier/mementoPool.js";

export const RESOLVE_BRIDGE_TARGET_CAPABILITY = "resolve-bridge-target";

export interface ResolveBridgeTargetInput {
  bridgeTargetContractCid: string;
  mementoPool: MementoPool;
}

export interface ResolvedProperty {
  cid: string;
  /** Precondition formula extracted from the contract memento's `pre` slot. */
  irFormula: IrFormula;
  contractName: string;
  outBinding: string;
}

export interface ResolveBridgeTargetOutput {
  resolved: ResolvedProperty | null;
  failureReason:
    | "not-in-pool"
    | "not-contract-variant"
    | "no-precondition"
    | null;
}

export interface MakeResolveBridgeTargetStageDeps {
  producerVersion?: string;
}

export function makeResolveBridgeTargetStage(
  deps: MakeResolveBridgeTargetStageDeps = {},
): Stage<ResolveBridgeTargetInput, ResolveBridgeTargetOutput> {
  const producedBy = deps.producerVersion ?? "resolveBridgeTarget@v3";

  return {
    name: "resolveBridgeTarget",
    producedBy,

    serializeInput(input) {
      return {
        bridgeTargetContractCid: input.bridgeTargetContractCid,
        poolCids: Object.keys(input.mementoPool.mementos).sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ResolveBridgeTargetOutput;
    },

    async run(input) {
      const envelope = input.mementoPool.mementos[input.bridgeTargetContractCid];
      if (!envelope) {
        return { resolved: null, failureReason: "not-in-pool" };
      }
      if (envelope.evidence?.kind !== "contract") {
        return { resolved: null, failureReason: "not-contract-variant" };
      }
      const ev = envelope.evidence as ContractEvidence;
      const pre = ev.body.pre as IrFormula | undefined;
      if (pre === undefined) {
        return { resolved: null, failureReason: "no-precondition" };
      }
      return {
        resolved: {
          cid: input.bridgeTargetContractCid,
          irFormula: pre,
          contractName: ev.body.contractName,
          outBinding: ev.body.outBinding,
        },
        failureReason: null,
      };
    },
  };
}
