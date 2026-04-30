/**
 * CompareFormulas stage — diff two IR formulas.
 *
 * Produces a structural delta between an `oldFormula` and a `newFormula`
 * for use by the weaken and strengthen workflows. The delta records:
 *   - whether the two property hashes differ at all (`changed`)
 *   - sub-formula additions (conjuncts/disjuncts present in new but not old)
 *   - sub-formula removals (present in old but not new)
 *   - the caller's declared change kind ("weaken" | "strengthen")
 *
 * v1 is STRUCTURAL: we hash sub-formulas using the canonicalizer and
 * compare hash multisets, not semantic implication. A real
 * weaken/strengthen verdict (new ⊆ old or new ⊇ old) needs an SMT
 * round-trip; that lands when the SMT-LIB translator wires through.
 * The Stage is the first-class slot for that future producer to plug
 * into; the structural delta is the placeholder that's still useful
 * today (it surfaces "what predicate text changed" precisely).
 *
 * The Stage's `mode` discriminates whether the caller is claiming
 * relaxation vs tightening — used by downstream consumers (the
 * workflow's terminal verdict) but not interpreted by this Stage.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md §"Change the
 *       invariant, the hash changes"
 */

import type { IrFormula } from "../../ir/formulas.js";
import { propertyHashFromFormula } from "../../canonicalizer/canonicalize.js";
import type { Stage } from "../types.js";

export const COMPARE_FORMULAS_CAPABILITY = "compare-formulas";

export type CompareMode = "weaken" | "strengthen";

export interface CompareFormulasStageInput {
  oldFormula: IrFormula;
  newFormula: IrFormula;
  /**
   * The caller's declared intent. Recorded in the delta so downstream
   * consumers (e.g. the workflow's terminal report) can render the
   * appropriate language.
   */
  mode: CompareMode;
}

export interface CompareFormulasDelta {
  /** Whether the two formulas have different property hashes. */
  changed: boolean;
  /** propertyHash of the old formula. */
  oldPropertyHash: string;
  /** propertyHash of the new formula. */
  newPropertyHash: string;
  /**
   * Top-level conjunct/disjunct hashes that exist in `new` but not in
   * `old`. Empty when the structural change is somewhere other than
   * the top-level boolean structure (e.g. a renamed variable, a
   * different operator deep inside a quantifier body).
   */
  addedSubFormulaHashes: string[];
  /**
   * Top-level conjunct/disjunct hashes that exist in `old` but not in
   * `new`. Same caveat as above.
   */
  removedSubFormulaHashes: string[];
  /** Echoed from the input. */
  mode: CompareMode;
}

export interface MakeCompareFormulasStageDeps {
  /** Override producer identity. Default: "compareFormulas@v1". */
  producerVersion?: string;
}

export function makeCompareFormulasStage(
  deps: MakeCompareFormulasStageDeps = {},
): Stage<CompareFormulasStageInput, CompareFormulasDelta> {
  const producedBy = deps.producerVersion ?? "compareFormulas@v1";

  return {
    name: "compareFormulas",
    producedBy,

    serializeInput(input) {
      return {
        oldFormula: input.oldFormula,
        newFormula: input.newFormula,
        mode: input.mode,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as CompareFormulasDelta;
    },

    async run(input) {
      return diffFormulas(input.oldFormula, input.newFormula, input.mode);
    },
  };
}

function diffFormulas(
  oldFormula: IrFormula,
  newFormula: IrFormula,
  mode: CompareMode,
): CompareFormulasDelta {
  const oldHash = propertyHashFromFormula(oldFormula);
  const newHash = propertyHashFromFormula(newFormula);

  const oldChildren = topLevelChildren(oldFormula).map(propertyHashFromFormula);
  const newChildren = topLevelChildren(newFormula).map(propertyHashFromFormula);

  const oldSet = multiset(oldChildren);
  const newSet = multiset(newChildren);

  const addedSubFormulaHashes: string[] = [];
  for (const [hash, count] of newSet) {
    const oldCount = oldSet.get(hash) ?? 0;
    for (let i = 0; i < count - oldCount; i++) addedSubFormulaHashes.push(hash);
  }
  const removedSubFormulaHashes: string[] = [];
  for (const [hash, count] of oldSet) {
    const newCount = newSet.get(hash) ?? 0;
    for (let i = 0; i < count - newCount; i++) removedSubFormulaHashes.push(hash);
  }

  return {
    changed: oldHash !== newHash,
    oldPropertyHash: oldHash,
    newPropertyHash: newHash,
    addedSubFormulaHashes: addedSubFormulaHashes.sort(),
    removedSubFormulaHashes: removedSubFormulaHashes.sort(),
    mode,
  };
}

function topLevelChildren(formula: IrFormula): IrFormula[] {
  if (formula.kind === "and") return formula.conjuncts;
  if (formula.kind === "or") return formula.disjuncts;
  return [formula];
}

function multiset(hashes: string[]): Map<string, number> {
  const out = new Map<string, number>();
  for (const h of hashes) out.set(h, (out.get(h) ?? 0) + 1);
  return out;
}
