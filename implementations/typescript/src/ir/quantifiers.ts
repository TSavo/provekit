/**
 * Quantifier builders — forAll, exists, forSome.
 *
 * Each takes a Sort and a callback that receives an IrTerm (the bound
 * variable) and returns an IrFormula. The callback is called immediately
 * at construction time; the resulting body is stored as pure data
 * (no JS closures in the IR tree).
 *
 * Variable names generated here are NOT semantically meaningful — the
 * AST canonicalizer replaces them with de Bruijn indices. We generate
 * stable monotonically-incrementing names to make debug output readable.
 */

import type { IrFormula, IrTerm, Sort, VarTerm } from "./formulas.js";

// ---------------------------------------------------------------------------
// Fresh variable name generator
// ---------------------------------------------------------------------------

let _counter = 0;

/** Reset counter — for test isolation only. */
export function _resetCounter(): void {
  _counter = 0;
}

function freshVarName(): string {
  return `_x${_counter++}`;
}

const SORT_HINT = Symbol.for("provekit.ir.sortHint");

function makeVar(name: string, sort?: Sort): VarTerm {
  const v: VarTerm = { kind: "var", name };
  if (sort !== undefined) {
    Object.defineProperty(v, SORT_HINT, {
      value: sort,
      enumerable: false,
      writable: true,
      configurable: true,
    });
  }
  return v;
}

// ---------------------------------------------------------------------------
// Quantifiers
// ---------------------------------------------------------------------------

/**
 * Universal quantifier.
 * `forAll(sort, body)` asserts that `body(x)` holds for all `x` of `sort`.
 */
export function forAll(sort: Sort, body: (x: IrTerm) => IrFormula): IrFormula {
  const name = freshVarName();
  const v = makeVar(name, sort);
  return {
    kind: "forall",
    name,
    sort,
    body: body(v),
  };
}

/**
 * Existential quantifier.
 * `exists(sort, body)` asserts that `body(x)` holds for some `x` of `sort`.
 */
export function exists(sort: Sort, body: (x: IrTerm) => IrFormula): IrFormula {
  const name = freshVarName();
  const v = makeVar(name, sort);
  return {
    kind: "exists",
    name,
    sort,
    body: body(v),
  };
}

/**
 * Bounded existential quantifier — `exists` restricted to a set domain.
 * Convenience alias: `forSome(domain, sort, body)` is sugar for
 * `exists(sort, x => and(member(x, domain), body(x)))`.
 *
 * Note: the `domain` here is a runtime IrTerm representing the set
 * expression. The sort describes the element sort of that set.
 */
export function forSome(
  domain: IrTerm,
  elementSort: Sort,
  body: (x: IrTerm) => IrFormula,
): IrFormula {
  const name = freshVarName();
  const v = makeVar(name, elementSort);
  const memberAtom: IrFormula = {
    kind: "atomic",
    name: "member",
    args: [v, domain],
  };
  return {
    kind: "exists",
    name,
    sort: elementSort,
    body: {
      kind: "and",
      operands: [memberAtom, body(v)],
    },
  };
}
