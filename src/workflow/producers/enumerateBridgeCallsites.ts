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
import type { ClaimEnvelope, PropertyEvidence, BridgeEvidence } from "../../claimEnvelope/types.js";
import type { IrFormula, IrTerm } from "../../ir/formulas.js";

export const ENUMERATE_BRIDGE_CALLSITES_CAPABILITY = "enumerate-bridge-callsites";

export interface EnumerateBridgeCallsitesInput {
  /** From load-all-proofs: every member memento, CID-keyed. */
  mementoPool: Record<string, ClaimEnvelope>;
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
        mementoPoolCids: Object.keys(input.mementoPool).sort(),
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
      for (const [cid, envelope] of Object.entries(input.mementoPool)) {
        if (envelope.evidence?.kind !== "property") continue;
        const ev = envelope.evidence as PropertyEvidence;
        const formula = ev.body.irFormula as IrFormula;
        const scope = ev.body.scope as { kind: string; name?: string };
        const propertyName = scope.name ?? cid.slice(0, 12);
        walkFormulaForBridgeCalls(formula, propertyName, cid, input.bridgesBySymbol, out);
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
      for (const c of formula.conjuncts) walkFormulaForBridgeCalls(c, propertyName, propertyCid, bridgesBySymbol, out);
      return;
    case "or":
      for (const d of formula.disjuncts) walkFormulaForBridgeCalls(d, propertyName, propertyCid, bridgesBySymbol, out);
      return;
    case "not":
      walkFormulaForBridgeCalls(formula.body, propertyName, propertyCid, bridgesBySymbol, out);
      return;
    case "implies":
      walkFormulaForBridgeCalls(formula.antecedent, propertyName, propertyCid, bridgesBySymbol, out);
      walkFormulaForBridgeCalls(formula.consequent, propertyName, propertyCid, bridgesBySymbol, out);
      return;
    case "forall":
    case "exists":
      walkFormulaForBridgeCalls(formula.predicate.body, propertyName, propertyCid, bridgesBySymbol, out);
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
