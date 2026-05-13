/**
 * Walks a set of formulas to collect Lean preamble declarations:
 *   - `axiom <UserSort> : Type` for every user-defined primitive sort
 *   - `axiom <ctorName> : Dom1 -> Dom2 -> ... -> Range` for every
 *     uninterpreted ctor that appears in any term position
 *   - `axiom <pred> : Arg1 -> Arg2 -> ... -> Prop` for atomic predicates
 *     that are not Lean built-ins
 *
 * Built-in sorts (Bool, Int, Real, String) and built-in atomic predicates
 * (=, !=, <, <=, >, >=) are not declared.
 *
 * The translator does NOT invent semantics: it only declares opaque
 * symbols. Axioms about those symbols are the kit's responsibility, the
 * same discipline the SMT translator follows.
 */

import type { IrFormula, IrTerm, Sort } from "../formulas.js";
import { collectUserSorts, emitSort } from "./sorts.js";

const BUILT_IN_PREDICATES = new Set([
  "=",
  "≠",
  "<",
  "≤",
  ">",
  "≥",
  "true",
  "false",
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
  /** Uninterpreted ctor signatures, sorted by name. */
  ctors: CtorSig[];
  /** Uninterpreted atomic predicates, sorted by name. */
  predicates: PredSig[];
}

interface CollectorState {
  userSorts: Set<string>;
  ctors: Map<string, CtorSig>;
  predicates: Map<string, PredSig>;
}

/**
 * Collect declarations from a list of formulas. Duplicate ctors with the
 * same arity are deduplicated; mismatched arities throw: the IR shouldn't
 * reuse a ctor name with two arities.
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
      `Lean emit: ctor "${name}" used with conflicting arities (${existing.argSorts.length} vs ${argSorts.length})`,
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
      `Lean emit: predicate "${name}" used with conflicting arities (${existing.argSorts.length} vs ${argSorts.length})`,
    );
  }
}

/**
 * Emit `axiom <Name> : Type` lines for user-defined sorts.
 *
 * Lean note: we use `axiom` rather than `variable` so the declarations are
 * self-contained at theorem statement scope (variables are section-scoped
 * and require a containing `section`). A kit that wants to provide
 * extensional content for these sorts can add axioms in a separate
 * preamble; the translator does not attempt that.
 */
export function emitSortDeclarations(decls: Declarations): string[] {
  return decls.userSorts.map((name) => `axiom ${name} : Type`);
}

/**
 * Emit `axiom <name> : Arg1 -> Arg2 -> ... -> Range` lines for ctors and
 * `axiom <name> : Arg1 -> Arg2 -> ... -> Prop` lines for predicates.
 *
 * Lean's curried-arrow notation is the natural shape: `f : Int -> Int`
 * declares a unary function from Int to Int. Zero-arg ctors collapse to
 * `axiom name : Range`, the constant form.
 */
export function emitFunctionDeclarations(decls: Declarations): string[] {
  const lines: string[] = [];
  for (const c of decls.ctors) {
    const sig = signatureArrow(c.argSorts, c.resultSort);
    lines.push(`axiom ${c.name} : ${sig}`);
  }
  for (const p of decls.predicates) {
    const sig = signatureArrow(p.argSorts, { kind: "primitive", name: "Prop" });
    lines.push(`axiom ${p.name} : ${sig}`);
  }
  return lines;
}

function signatureArrow(argSorts: Sort[], resultSort: Sort): string {
  // The result sort is rendered by emitSort except for the synthetic
  // "Prop" primitive we use as a sentinel for predicate signatures.
  const resultText =
    resultSort.kind === "primitive" && resultSort.name === "Prop"
      ? "Prop"
      : emitSort(resultSort);
  if (argSorts.length === 0) {
    return resultText;
  }
  const parts = argSorts.map((s) => emitSort(s));
  parts.push(resultText);
  return parts.join(" -> ");
}
