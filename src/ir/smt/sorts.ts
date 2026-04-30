/**
 * Sort → SMT-LIB sort string mapping.
 *
 * Built-in primitive sorts map to their SMT-LIB counterparts. Any other
 * primitive name is treated as a user-declared sort and surfaces in
 * `collectUserSorts` so it can be `(declare-sort Name 0)`'d in the
 * problem preamble.
 *
 * Set, tuple, and function sorts are emitted as parametric SMT-LIB types.
 * Sets use `(Set T)`, tuples use `(Tuple T1 T2 ...)`, function sorts in
 * argument positions are flattened into curried form by callers when
 * needed (SMT-LIB doesn't have first-class function sorts in the AUFLIA
 * fragment; ALL logic supports them via parametric arrays for some
 * solvers — for now we declare uninterpreted function symbols by their
 * domain/range, not by their function-sort).
 */

import type { Sort } from "../formulas.js";

const PRIMITIVE_TO_SMT: Record<string, string> = {
  Bool: "Bool",
  Int: "Int",
  Real: "Real",
  String: "String",
};

/** Sort names that map to SMT-LIB built-ins. */
const BUILT_IN_PRIMITIVES = new Set(Object.keys(PRIMITIVE_TO_SMT));

/**
 * Render a Sort as the SMT-LIB sort expression that names it.
 * User-defined primitives are emitted as bare identifiers (the caller
 * is responsible for `(declare-sort ...)`).
 */
export function emitSort(sort: Sort): string {
  switch (sort.kind) {
    case "primitive": {
      const mapped = PRIMITIVE_TO_SMT[sort.name];
      return mapped ?? sort.name;
    }
    case "set":
      return `(Set ${emitSort(sort.element)})`;
    case "tuple":
      return `(Tuple ${sort.elements.map(emitSort).join(" ")})`;
    case "function":
      // Function sorts aren't first-class in plain SMT-LIB. We render
      // them as `(-> dom1 dom2 ... range)` which mirrors the SMT-LIB
      // higher-order extension; consumers that don't support it should
      // not place function sorts in atomic positions.
      return `(-> ${sort.domain.map(emitSort).join(" ")} ${emitSort(sort.range)})`;
  }
}

/**
 * Walk a sort and collect all user-defined primitive sort names that
 * need `(declare-sort Name 0)` in the problem preamble.
 */
export function collectUserSorts(sort: Sort, out: Set<string>): void {
  switch (sort.kind) {
    case "primitive":
      if (!BUILT_IN_PRIMITIVES.has(sort.name)) {
        out.add(sort.name);
      }
      return;
    case "set":
      collectUserSorts(sort.element, out);
      return;
    case "tuple":
      for (const e of sort.elements) collectUserSorts(e, out);
      return;
    case "function":
      for (const d of sort.domain) collectUserSorts(d, out);
      collectUserSorts(sort.range, out);
      return;
  }
}

export function isBuiltInPrimitive(name: string): boolean {
  return BUILT_IN_PRIMITIVES.has(name);
}
