/**
 * Walks a set of formulas to collect the SMT-LIB declarations the
 * preamble needs:
 *
 *   - `(declare-sort UserSort 0)` for every user-defined primitive sort
 *   - `(declare-fun ctorName (DomSort1 ...) RangeSort)` for every
 *     uninterpreted ctor that appears in any term position.
 *
 * Built-in sorts (Bool, Int, Real, String) and built-in atomic
 * predicates are not declared. Atomic predicates that aren't built-in
 * are declared as `(declare-fun pred (Sort1 ...) Bool)`.
 *
 * The translator does NOT invent semantics: it only declares symbols.
 * Axioms about those symbols are the kit's responsibility.
 */

import type { IrFormula, IrTerm, Sort } from "../formulas.js";
import { collectUserSorts, emitSort } from "./sorts.js";

/**
 * Atomic predicates that map to SMT-LIB built-in operators and need no
 * `(declare-fun ...)` in the preamble. Notably this does NOT include
 * `member` and `subset`: SMT-LIB has no portable base names for them
 * (Z3 spells them `set.member` / `set.subset`; CVC5 differs again), so
 * we treat them as uninterpreted predicates whose semantics come from
 * the kit's axioms: consistent with the translator's "don't invent
 * semantics" discipline.
 */
const BUILT_IN_PREDICATES = new Set([
  "=",
  "≠",
  "<",
  "≤",
  ">",
  "≥",
  "true",
  "false",
  // SMT-LIB BV theory comparison predicates: declared by the theory, not
  // by us. Treat them as built-ins so collectDeclarations skips emitting
  // a (declare-fun ...) for them.
  "bvult",
  "bvule",
  "bvugt",
  "bvuge",
  "bvslt",
  "bvsle",
  "bvsgt",
  "bvsge",
]);

/**
 * SMT-LIB BV theory term operators (`bvadd`, `bvxor`, `concat`, ...) plus
 * the indexed `extract` operator. These are theory-provided ctors with
 * no `(declare-fun ...)` requirement.
 */
const BUILT_IN_CTORS = new Set([
  "bvadd",
  "bvsub",
  "bvmul",
  "bvudiv",
  "bvurem",
  "bvshl",
  "bvlshr",
  "bvashr",
  "bvor",
  "bvand",
  "bvxor",
  "bvnot",
  "bvneg",
  "concat",
  "extract",
]);

interface CtorSig {
  name: string;
  argSorts: Sort[];
  resultSort: Sort;
}

interface PredSig {
  name: string;
  argSorts: Sort[];
}

export interface Declarations {
  /** User-declared primitive sort names (alphabetically stable order). */
  userSorts: string[];
  /** Uninterpreted ctor signatures keyed by ctor name. */
  ctors: CtorSig[];
  /** Uninterpreted atomic predicates keyed by name. */
  predicates: PredSig[];
}

interface CollectorState {
  userSorts: Set<string>;
  ctors: Map<string, CtorSig>;
  predicates: Map<string, PredSig>;
}

/**
 * Collect declarations from a list of formulas. Duplicate ctors with
 * the same signature are deduplicated; mismatched signatures throw: * the IR shouldn't reuse a ctor name with two arities.
 */
export function collectDeclarations(formulas: IrFormula[]): Declarations {
  const state: CollectorState = {
    userSorts: new Set<string>(),
    ctors: new Map<string, CtorSig>(),
    predicates: new Map<string, PredSig>(),
  };
  for (const f of formulas) {
    walkFormula(f, state);
  }
  return {
    userSorts: [...state.userSorts].sort(),
    ctors: [...state.ctors.values()].sort((a, b) => a.name.localeCompare(b.name)),
    predicates: [...state.predicates.values()].sort((a, b) =>
      a.name.localeCompare(b.name),
    ),
  };
}

const REF_SORT: Sort = { kind: "primitive", name: "Ref" };
const SORT_HINT = Symbol.for("provekit.ir.sortHint");

function inferTermSort(t: IrTerm, scope: Map<string, Sort>): Sort {
  if (t.kind === "const") return t.sort;
  if (t.kind === "var") return scope.get(t.name) ?? REF_SORT;
  const v = (t as unknown as Record<symbol, unknown>)[SORT_HINT];
  return (v as Sort | undefined) ?? REF_SORT;
}

function walkFormula(
  formula: IrFormula,
  state: CollectorState,
  scope: Map<string, Sort> = new Map(),
): void {
  switch (formula.kind) {
    case "forall":
    case "exists": {
      collectUserSorts(formula.sort, state.userSorts);
      const next = new Map(scope);
      next.set(formula.name, formula.sort);
      walkFormula(formula.body, state, next);
      return;
    }
    case "and":
    case "or":
    case "implies":
      for (const o of formula.operands) walkFormula(o, state, scope);
      return;
    case "not":
      walkFormula(formula.operands[0]!, state, scope);
      return;
    case "atomic": {
      for (const t of formula.args) walkTerm(t, state, scope);
      if (!BUILT_IN_PREDICATES.has(formula.name)) {
        const argSorts = formula.args.map((a) => inferTermSort(a, scope));
        recordPredicate(state, formula.name, argSorts);
      }
      return;
    }
  }
}

function walkTerm(
  term: IrTerm,
  state: CollectorState,
  scope: Map<string, Sort>,
): void {
  collectUserSorts(inferTermSort(term, scope), state.userSorts);
  if (term.kind === "ctor") {
    for (const a of term.args) walkTerm(a, state, scope);
    if (BUILT_IN_CTORS.has(term.name)) return;
    recordCtor(
      state,
      term.name,
      term.args.map((a) => inferTermSort(a, scope)),
      inferTermSort(term, scope),
    );
  }
}

function recordCtor(
  state: CollectorState,
  name: string,
  argSorts: Sort[],
  resultSort: Sort,
): void {
  const existing = state.ctors.get(name);
  if (!existing) {
    state.ctors.set(name, { name, argSorts, resultSort });
    return;
  }
  if (existing.argSorts.length !== argSorts.length) {
    throw new Error(
      `SMT emit: ctor "${name}" used with conflicting arities (${existing.argSorts.length} vs ${argSorts.length})`,
    );
  }
}

function recordPredicate(
  state: CollectorState,
  name: string,
  argSorts: Sort[],
): void {
  const existing = state.predicates.get(name);
  if (!existing) {
    state.predicates.set(name, { name, argSorts });
    return;
  }
  if (existing.argSorts.length !== argSorts.length) {
    throw new Error(
      `SMT emit: predicate "${name}" used with conflicting arities (${existing.argSorts.length} vs ${argSorts.length})`,
    );
  }
}

/** Emit `(declare-sort Name 0)` for each user-defined sort. */
export function emitSortDeclarations(decls: Declarations): string[] {
  return decls.userSorts.map((name) => `(declare-sort ${name} 0)`);
}

/** Emit `(declare-fun name (Args ...) Range)` for each ctor. */
export function emitFunctionDeclarations(decls: Declarations): string[] {
  const lines: string[] = [];
  for (const c of decls.ctors) {
    const args = c.argSorts.map(emitSort).join(" ");
    lines.push(`(declare-fun ${c.name} (${args}) ${emitSort(c.resultSort)})`);
  }
  for (const p of decls.predicates) {
    const args = p.argSorts.map(emitSort).join(" ");
    lines.push(`(declare-fun ${p.name} (${args}) Bool)`);
  }
  return lines;
}
