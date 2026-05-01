/**
 * Pass 1: de Bruijn index replacement.
 *
 * Pre-condition: input is an IrFormula (maximal-uniformity grammar) with
 *   named-variable references (var.name strings tied to enclosing binders).
 * Post-condition: output is a DeBruijnFormula where every `var` node has
 *   its `name` annotated with the de Bruijn index of its binder. The
 *   binder sort is propagated onto the var node so downstream passes can
 *   build CanonicalVar nodes without re-walking. CtorTerm carries no
 *   declared sort; downstream emitters that need a result sort consult
 *   the kit registry or fall back to the Ref sentinel.
 *
 * De Bruijn index convention: index 0 refers to the nearest enclosing
 * binder. Index k refers to the binder k levels up. This erases variable
 * names and makes alpha-equivalent formulas structurally identical.
 */

import type { IrFormula, IrTerm, Sort } from "../irFormula.js";

// -----------------------------------------------------------------------
// Internal types: IrFormula annotated with de Bruijn indices
// -----------------------------------------------------------------------

const REF_SORT: Sort = { kind: "primitive", name: "Ref" };

export type DeBruijnTerm =
  | { kind: "var"; name: string; sort: Sort; deBruijn: number }
  | { kind: "const"; value: unknown; sort: Sort }
  | { kind: "ctor"; name: string; args: DeBruijnTerm[]; sort: Sort }
  | { kind: "lambda"; paramName: string; paramSort: Sort; body: DeBruijnTerm; sort: Sort }
  | { kind: "let"; bindings: DeBruijnBinding[]; body: DeBruijnTerm; sort: Sort };

export type DeBruijnBinding = { name: string; boundTerm: DeBruijnTerm };

export type DeBruijnFormula =
  | { kind: "forall"; sort: Sort; varName: string; body: DeBruijnFormula }
  | { kind: "exists"; sort: Sort; varName: string; body: DeBruijnFormula }
  | { kind: "and"; conjuncts: DeBruijnFormula[] }
  | { kind: "or"; disjuncts: DeBruijnFormula[] }
  | { kind: "not"; body: DeBruijnFormula }
  | { kind: "implies"; antecedent: DeBruijnFormula; consequent: DeBruijnFormula }
  | { kind: "atomic"; predicate: string; args: DeBruijnTerm[] }
  | { kind: "choice"; sort: Sort; varName: string; body: DeBruijnFormula };

// -----------------------------------------------------------------------
// Implementation
// -----------------------------------------------------------------------

interface BinderEntry {
  name: string;
  sort: Sort;
}

const SORT_HINT = Symbol.for("provekit.ir.sortHint");

function readSortHint(t: IrTerm): Sort | undefined {
  const v = (t as unknown as Record<symbol, unknown>)[SORT_HINT];
  return (v as Sort | undefined) ?? undefined;
}

/**
 * Replace named-variable references with de Bruijn indices.
 * `stack` maps binder entries (name + sort) to their depth (index 0 = innermost).
 */
export function applyDeBruijn(formula: IrFormula, stack: BinderEntry[] = []): DeBruijnFormula {
  switch (formula.kind) {
    case "forall":
    case "exists": {
      const newStack: BinderEntry[] = [{ name: formula.name, sort: formula.sort }, ...stack];
      return {
        kind: formula.kind,
        sort: formula.sort,
        varName: formula.name,
        body: applyDeBruijn(formula.body, newStack),
      };
    }

    case "and":
      return { kind: "and", conjuncts: formula.operands.map((c) => applyDeBruijn(c, stack)) };

    case "or":
      return { kind: "or", disjuncts: formula.operands.map((d) => applyDeBruijn(d, stack)) };

    case "not":
      return { kind: "not", body: applyDeBruijn(formula.operands[0]!, stack) };

    case "implies":
      return {
        kind: "implies",
        antecedent: applyDeBruijn(formula.operands[0]!, stack),
        consequent: applyDeBruijn(formula.operands[1]!, stack),
      };

    case "atomic":
      return {
        kind: "atomic",
        predicate: formula.name,
        args: formula.args.map((t) => applyDeBruijnTerm(t, stack)),
      };

    case "choice": {
      const newStack: BinderEntry[] = [{ name: formula.varName, sort: formula.sort }, ...stack];
      return {
        kind: "choice",
        sort: formula.sort,
        varName: formula.varName,
        body: applyDeBruijn(formula.body, newStack),
      };
    }
  }
}

function applyDeBruijnTerm(term: IrTerm, stack: BinderEntry[]): DeBruijnTerm {
  switch (term.kind) {
    case "var": {
      const idx = stack.findIndex((b) => b.name === term.name);
      if (idx === -1) {
        throw new Error(
          `de Bruijn: unbound variable "${term.name}". Stack: [${stack.map((b) => b.name).join(", ")}]`,
        );
      }
      return { kind: "var", name: term.name, sort: stack[idx]!.sort, deBruijn: idx };
    }
    case "const":
      return { kind: "const", value: term.value, sort: term.sort };
    case "ctor":
      return {
        kind: "ctor",
        name: term.name,
        args: term.args.map((a) => applyDeBruijnTerm(a, stack)),
        sort: readSortHint(term) ?? REF_SORT,
      };

    case "lambda": {
      const newStack: BinderEntry[] = [{ name: term.paramName, sort: term.paramSort }, ...stack];
      return {
        kind: "lambda",
        paramName: term.paramName,
        paramSort: term.paramSort,
        body: applyDeBruijnTerm(term.body, newStack),
        sort: readSortHint(term) ?? REF_SORT,
      };
    }

    case "let": {
      const bindings: DeBruijnBinding[] = [];
      let currentStack = stack;
      for (const b of term.bindings) {
        bindings.push({
          name: b.name,
          boundTerm: applyDeBruijnTerm(b.boundTerm, currentStack),
        });
        currentStack = [{ name: b.name, sort: REF_SORT }, ...currentStack];
      }
      return {
        kind: "let",
        bindings,
        body: applyDeBruijnTerm(term.body, currentStack),
        sort: readSortHint(term) ?? REF_SORT,
      };
    }
  }
}
