/**
 * Sort -> Lean 4 sort string mapping.
 *
 * Built-in primitive sorts map to their Lean counterparts. Other primitive
 * names are treated as user-declared opaque types and surface in
 * collectUserSorts so the preamble can declare them via `axiom <Name> : Type`.
 *
 * Lean's logic is much richer than FOL but the translator only targets the
 * propositional + first-order fragment. Set/tuple/function sorts have no
 * canonical mapping in plain prop logic without committing to a Mathlib
 * dependency (Set, Prod, function arrows). The translator throws structured
 * errors on those rather than silently mistranslating.
 */

import type { Sort } from "../formulas.js";

const PRIMITIVE_TO_LEAN: Record<string, string> = {
  Bool: "Bool",
  Int: "Int",
  Real: "Real",
  String: "String",
};

const BUILT_IN_PRIMITIVES = new Set(Object.keys(PRIMITIVE_TO_LEAN));

export class LeanUnsupportedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "LeanUnsupportedError";
  }
}

/**
 * Render a Sort as the Lean 4 type expression that names it. User-defined
 * primitives are emitted as bare identifiers (the caller is responsible for
 * declaring them as opaque types in the preamble).
 *
 * Tuple, set, and function sorts throw — they require Mathlib lemmas that
 * the translator does not commit to.
 */
export function emitSort(sort: Sort): string {
  switch (sort.kind) {
    case "primitive": {
      const mapped = PRIMITIVE_TO_LEAN[sort.name];
      return mapped ?? sort.name;
    }
    case "set":
      throw new LeanUnsupportedError(
        `Lean translator: set sorts are out of scope for the FOL fragment (Mathlib's Set is not committed to). Got (Set ${sortKindLabel(sort.element)}).`,
      );
    case "tuple":
      throw new LeanUnsupportedError(
        "Lean translator: tuple sorts have no plain-Lean encoding without Mathlib's Prod. Express via separately-quantified components.",
      );
    case "function":
      throw new LeanUnsupportedError(
        "Lean translator: function sorts are not first-class in the FOL fragment. Declare an opaque function symbol via `axiom f : T1 -> T2` in the kit and use it as a ctor.",
      );
  }
}

function sortKindLabel(sort: Sort): string {
  if (sort.kind === "primitive") return sort.name;
  return sort.kind;
}

/**
 * Walk a sort and collect user-defined primitive sort names that need an
 * `axiom <Name> : Type` declaration in the preamble.
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
