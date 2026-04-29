/**
 * The `property()` constructor — the top-level entry point for
 * declaring IR properties. Every property is a named claim about
 * a scoped piece of code, expressed as an IrFormula.
 */

import type {
  Bindings,
  BindingScope,
  CompilationHint,
  IrFormula,
  IrTerm,
  Sort,
} from "./formulas.js";

// Re-export types that consumers need to spell `Property` etc.
export type { Bindings, BindingScope, CompilationHint };

// ---------------------------------------------------------------------------
// Property interface
// ---------------------------------------------------------------------------

export interface Property {
  name: string;
  scope: BindingScope;
  bindings: Bindings;
  formula: IrFormula;
  hint?: CompilationHint;
}

// ---------------------------------------------------------------------------
// Property constructor
// ---------------------------------------------------------------------------

/**
 * Declare a named property.
 *
 * `bindings` maps variable names to Sort values. When `formula` is a
 * function, it receives an object of `IrTerm` handles keyed by the
 * same names — one `var`-term per binding. This lets authors write:
 *
 *   formula: ({ b }) => assert.notEqual(b, 0)
 *
 * The term handles have the same names and sorts as the bindings map.
 *
 * If `formula` is already an `IrFormula` value (not a function), it
 * is used as-is.
 */
export function property(spec: {
  name: string;
  scope: BindingScope;
  bindings: Bindings;
  formula: ((bindings: Record<string, IrTerm>) => IrFormula) | IrFormula;
  hint?: CompilationHint;
}): Property {
  let formula: IrFormula;

  if (typeof spec.formula === "function") {
    // Build IrTerm handles from the bindings map.
    const termHandles: Record<string, IrTerm> = {};
    for (const [name, sort] of Object.entries(spec.bindings)) {
      termHandles[name] = buildVarTerm(name, sort);
    }
    formula = spec.formula(termHandles);
  } else {
    formula = spec.formula;
  }

  return {
    name: spec.name,
    scope: spec.scope,
    bindings: spec.bindings,
    formula,
    ...(spec.hint !== undefined ? { hint: spec.hint } : {}),
  };
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function buildVarTerm(name: string, sort: Sort): IrTerm {
  return { kind: "var", name, sort };
}
