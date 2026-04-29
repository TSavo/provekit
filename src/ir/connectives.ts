/**
 * Logical connective builders — and, or, not, implies, iff.
 */

import type { IrFormula } from "./formulas.js";

/** Conjunction. All conjuncts must hold. */
export function and(...conjuncts: IrFormula[]): IrFormula {
  if (conjuncts.length === 0) {
    // vacuously true
    return { kind: "atomic", predicate: "true", args: [] };
  }
  if (conjuncts.length === 1) {
    return conjuncts[0];
  }
  return { kind: "and", conjuncts };
}

/** Disjunction. At least one disjunct must hold. */
export function or(...disjuncts: IrFormula[]): IrFormula {
  if (disjuncts.length === 0) {
    // vacuously false
    return { kind: "atomic", predicate: "false", args: [] };
  }
  if (disjuncts.length === 1) {
    return disjuncts[0];
  }
  return { kind: "or", disjuncts };
}

/** Negation. */
export function not(formula: IrFormula): IrFormula {
  return { kind: "not", body: formula };
}

/** Implication: antecedent → consequent. */
export function implies(antecedent: IrFormula, consequent: IrFormula): IrFormula {
  return { kind: "implies", antecedent, consequent };
}

/** Biconditional (if and only if): a ↔ b. */
export function iff(a: IrFormula, b: IrFormula): IrFormula {
  return { kind: "iff", left: a, right: b };
}
