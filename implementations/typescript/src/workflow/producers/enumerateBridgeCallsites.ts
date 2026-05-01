/**
 * enumerate-bridge-callsites — Stage 2 of the bridge enforcement
 * workflow.
 *
 * Input: the unified memento pool from load-all-proofs (Stage 1).
 * Walks every property memento in the pool; finds `ctor` nodes whose
 * name matches a bridge envelope's sourceSymbol; emits a
 * BridgeCallSite for each match.
 *
 * Args at the call site are already in IR form (the kit emitted them
 * when its invariant code ran); no source lifting needed.
 */

import type { Stage } from "../types.js";
import type { ClaimEnvelope, ContractEvidence, BridgeEvidence } from "../../claimEnvelope/types.js";
import type { IrFormula, IrTerm } from "../../ir/formulas.js";
import type { MementoPool } from "../../verifier/mementoPool.js";

export const ENUMERATE_BRIDGE_CALLSITES_CAPABILITY = "enumerate-bridge-callsites";

export interface EnumerateBridgeCallsitesInput {
  /** From load-all-proofs: the memento pool (mementos IS verification). */
  mementoPool: MementoPool;
  /** From load-all-proofs: bridge envelopes indexed by sourceSymbol (IR name). */
  bridgesBySymbol: Record<string, ClaimEnvelope>;
}

export interface BridgeCallSite {
  bridgeIrName: string;
  bridgeTargetContractCid: string;
  bridgeSourceLayer: string;
  bridgeTargetLayer: string;
  /** Name of the property memento (from its scope) the call site appears in. */
  propertyName: string;
  /** CID of the property memento containing this call site. */
  propertyCid: string;
  /** The args the bridge primitive was called with (kit-emitted IR). */
  argTerms: IrTerm[];
}

export interface EnumerateBridgeCallsitesOutput {
  callsites: BridgeCallSite[];
}

export interface MakeEnumerateBridgeCallsitesStageDeps {
  producerVersion?: string;
}

export function makeEnumerateBridgeCallsitesStage(
  deps: MakeEnumerateBridgeCallsitesStageDeps = {},
): Stage<EnumerateBridgeCallsitesInput, EnumerateBridgeCallsitesOutput> {
  const producedBy = deps.producerVersion ?? "enumerateBridgeCallsites@v3";

  return {
    name: "enumerateBridgeCallsites",
    producedBy,

    serializeInput(input) {
      // Cache key: sorted list of member CIDs in the pool + sorted list
      // of bridge symbols. Pool contents change → cache invalidates.
      return {
        mementoPoolCids: Object.keys(input.mementoPool.mementos).sort(),
        bridgeSymbols: Object.keys(input.bridgesBySymbol).sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as EnumerateBridgeCallsitesOutput;
    },

    async run(input) {
      const out: BridgeCallSite[] = [];
      // The memento IS the verification; we walk the pool to find
      // contract mementos that reference bridges via their formulas.
      for (const [cid, envelope] of Object.entries(input.mementoPool.mementos)) {
        if (envelope.evidence?.kind !== "contract") continue;
        const ev = envelope.evidence as ContractEvidence;
        const propertyName = ev.body.contractName ?? cid.slice(0, 12);
        // Walk every present formula slot — pre / post / inv — for bridge call sites.
        for (const slot of ["pre", "post", "inv"] as const) {
          const f = ev.body[slot] as IrFormula | undefined;
          if (f) {
            walkFormulaForBridgeCalls(f, propertyName, cid, input.bridgesBySymbol, out);
          }
        }
      }
      return { callsites: out };
    },
  };
}

function walkFormulaForBridgeCalls(
  formula: IrFormula,
  propertyName: string,
  propertyCid: string,
  bridgesBySymbol: Record<string, ClaimEnvelope>,
  out: BridgeCallSite[],
): void {
  switch (formula.kind) {
    case "atomic":
      for (const arg of formula.args) walkTermForBridgeCalls(arg, propertyName, propertyCid, bridgesBySymbol, out);
      return;
    case "and":
    case "or":
    case "not":
    case "implies":
      for (const o of formula.operands) walkFormulaForBridgeCalls(o, propertyName, propertyCid, bridgesBySymbol, out);
      return;
    case "forall":
    case "exists":
      walkFormulaForBridgeCalls(formula.body, propertyName, propertyCid, bridgesBySymbol, out);
      return;
  }
}

function walkTermForBridgeCalls(
  term: IrTerm,
  propertyName: string,
  propertyCid: string,
  bridgesBySymbol: Record<string, ClaimEnvelope>,
  out: BridgeCallSite[],
): void {
  if (term.kind === "ctor") {
    const bridgeEnvelope = bridgesBySymbol[term.name];
    if (bridgeEnvelope) {
      const ev = bridgeEnvelope.evidence as BridgeEvidence;
      out.push({
        bridgeIrName: ev.body.sourceSymbol,
        bridgeTargetContractCid: ev.body.targetContractCid,
        bridgeSourceLayer: ev.body.sourceLayer,
        bridgeTargetLayer: ev.body.targetLayer,
        propertyName,
        propertyCid,
        argTerms: term.args,
      });
    }
    for (const a of term.args) walkTermForBridgeCalls(a, propertyName, propertyCid, bridgesBySymbol, out);
  }
}
