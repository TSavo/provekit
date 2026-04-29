/**
 * Sort values — runtime objects matching the Sort discriminated union
 * from formulas.ts. These are the "type tokens" used in quantifier
 * and property declarations.
 */

import type { Sort } from "./formulas.js";

// ---------------------------------------------------------------------------
// Primitive sorts (singleton objects)
// ---------------------------------------------------------------------------

export const Bool: Sort = { kind: "primitive", name: "Bool" };
export const Int: Sort = { kind: "primitive", name: "Int" };
export const Real: Sort = { kind: "primitive", name: "Real" };
export const String: Sort = { kind: "primitive", name: "String" };
export const Ref: Sort = { kind: "primitive", name: "Ref" };
export const Node: Sort = { kind: "primitive", name: "Node" };
export const Edge: Sort = { kind: "primitive", name: "Edge" };

// ---------------------------------------------------------------------------
// Constructed sorts
// ---------------------------------------------------------------------------

/** Construct a set sort. */
export function SetOf(element: Sort): Sort {
  return { kind: "set", element };
}

/** Construct a tuple sort. */
export function TupleOf(...elements: Sort[]): Sort {
  return { kind: "tuple", elements };
}

/** Construct a function sort. */
export function FuncOf(domain: Sort[], range: Sort): Sort {
  return { kind: "function", domain, range };
}
