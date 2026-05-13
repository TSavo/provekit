/**
 * Sort values: runtime objects matching the Sort discriminated union
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
// Bitvector sorts: SMT-LIB BV theory.
//
// `BV(width)` constructs a bitvector sort of the given bit-width. Common
// widths are exposed as named singletons for ergonomics. Any positive
// integer width is valid; the SMT-LIB translator renders these as
// `(_ BitVec N)`.
// ---------------------------------------------------------------------------

export function BV(width: number): Sort {
  if (!Number.isInteger(width) || width <= 0) {
    throw new Error(`BV width must be a positive integer, got ${width}`);
  }
  return { kind: "bitvec", width };
}

export const BV8: Sort = BV(8);
export const BV16: Sort = BV(16);
export const BV32: Sort = BV(32);
export const BV64: Sort = BV(64);
export const BV128: Sort = BV(128);
export const BV256: Sort = BV(256);

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
export function FuncOf(args: Sort[], ret: Sort): Sort {
  return { kind: "function", args, return: ret };
}

/** Construct a region sort for lifetime parameter tracking. */
export function RegionOf(name: string): Sort {
  return { kind: "region", name };
}
