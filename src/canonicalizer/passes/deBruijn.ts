/**
 * Pass 1: de Bruijn index replacement.
 *
 * Pre-condition: input is an IrFormula with named-variable references
 *   (var.name strings tied to enclosing lambda binders).
 * Post-condition: output is an IrFormula where every `var` node has its
 *   `name` replaced by the de Bruijn index of its binder. The `name`
 *   field is retained in the structure but becomes semantically irrelevant;
 *   downstream passes replace it with an index-only CanonicalVar.
 *
 * De Bruijn index convention: index 0 refers to the nearest enclosing
 * binder. Index k refers to the binder k levels up. This erases variable
 * names and makes alpha-equivalent formulas structurally identical.
 *
 * Example:
 *   forAll("a", Int, forAll("b", Int, equal(a, b)))
 *   → forall(Int, forall(Int, equal(var@1, var@0)))
 */

import type { IrFormula, IrTerm } from "../irFormula.js";

// -----------------------------------------------------------------------
// Internal types: IrFormula annotated with de Bruijn indices
// -----------------------------------------------------------------------

/**
 * An IrTerm after de Bruijn replacement. For `var` nodes, `deBruijn` is
 * set; `name` is preserved for debugging but not used semantically.
 */
export type DeBruijnTerm =
  | { kind: "var"; name: string; sort: import("../irFormula.js").Sort; deBruijn: number }
  | { kind: "const"; value: unknown; sort: import("../irFormula.js").Sort }
  | { kind: "ctor"; name: string; args: DeBruijnTerm[]; sort: import("../irFormula.js").Sort };

export type DeBruijnFormula =
  | { kind: "forall"; sort: import("../irFormula.js").Sort; varName: string; body: DeBruijnFormula }
  | { kind: "exists"; sort: import("../irFormula.js").Sort; varName: string; body: DeBruijnFormula }
  | { kind: "and"; conjuncts: DeBruijnFormula[] }
  | { kind: "or"; disjuncts: DeBruijnFormula[] }
  | { kind: "not"; body: DeBruijnFormula }
  | { kind: "implies"; antecedent: DeBruijnFormula; consequent: DeBruijnFormula }
  | { kind: "atomic"; predicate: string; args: DeBruijnTerm[] };

// -----------------------------------------------------------------------
// Implementation
// -----------------------------------------------------------------------

/**
 * Replace named-variable references with de Bruijn indices.
 * `stack` maps variable names to their depth (index 0 = innermost).
 */
export function applyDeBruijn(formula: IrFormula, stack: string[] = []): DeBruijnFormula {
  switch (formula.kind) {
    case "forall":
    case "exists": {
      const { varName, sort, body } = formula.predicate;
      const newStack = [varName, ...stack];
      return {
        kind: formula.kind,
        sort,
        varName,
        body: applyDeBruijn(body, newStack),
      };
    }

    case "and":
      return { kind: "and", conjuncts: formula.conjuncts.map((c) => applyDeBruijn(c, stack)) };

    case "or":
      return { kind: "or", disjuncts: formula.disjuncts.map((d) => applyDeBruijn(d, stack)) };

    case "not":
      return { kind: "not", body: applyDeBruijn(formula.body, stack) };

    case "implies":
      return {
        kind: "implies",
        antecedent: applyDeBruijn(formula.antecedent, stack),
        consequent: applyDeBruijn(formula.consequent, stack),
      };

    case "atomic":
      return {
        kind: "atomic",
        predicate: formula.predicate,
        args: formula.args.map((t) => applyDeBruijnTerm(t, stack)),
      };
  }
}

function applyDeBruijnTerm(term: IrTerm, stack: string[]): DeBruijnTerm {
  switch (term.kind) {
    case "var": {
      const idx = stack.indexOf(term.name);
      if (idx === -1) {
        throw new Error(
          `de Bruijn: unbound variable "${term.name}". Stack: [${stack.join(", ")}]`,
        );
      }
      return { kind: "var", name: term.name, sort: term.sort, deBruijn: idx };
    }
    case "const":
      return { kind: "const", value: term.value, sort: term.sort };
    case "ctor":
      return {
        kind: "ctor",
        name: term.name,
        args: term.args.map((a) => applyDeBruijnTerm(a, stack)),
        sort: term.sort,
      };
  }
}
