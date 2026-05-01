/**
 * IR substitution — instantiate a forall-quantified IrFormula at a
 * specific term.
 *
 * Used by the bridge-enforcement engine: given a precondition formula
 * `forall s: String. nonempty(s)` from a property memento, and a
 * call-site argument (e.g., the Const `""` from `parseInt("")`),
 * produce the obligation formula `nonempty("")` — which the solver
 * dispatcher then decides.
 *
 * Today's IR uses NAMED bound variables. Substitution by name match is
 * correct provided the substituted term has no free variables that
 * collide with names bound deeper in the formula. For the v1 use case
 * (string literals, integer literals, other Const terms with no free
 * variables), this is automatically safe. A more general substitution
 * that handles variable-substitution-into-a-variable-binding would
 * require alpha-renaming; deferred.
 */

import type { IrFormula, IrTerm } from "./formulas.js";

export class SubstituteError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SubstituteError";
  }
}

/**
 * Drop the outermost `forall` and substitute its bound variable with
 * the supplied term throughout the body. Returns the substituted
 * formula.
 *
 * Throws SubstituteError if the input formula is not a forall, or if
 * the substituted term has free variables that collide with deeper
 * bindings (capture would silently change meaning).
 */
export function instantiateOutermostForall(
  formula: IrFormula,
  term: IrTerm,
): IrFormula {
  if (formula.kind !== "forall") {
    throw new SubstituteError(
      `instantiateOutermostForall expected a forall, got ${formula.kind}`,
    );
  }
  const boundName = formula.name;

  // Capture safety: term must not contain free vars whose names are
  // bound by quantifiers inside the body.
  const termFreeVars = collectFreeVarNames(term);
  if (termFreeVars.size > 0) {
    const innerBindings = collectInnerBoundVarNames(formula.body);
    for (const v of termFreeVars) {
      if (innerBindings.has(v)) {
        throw new SubstituteError(
          `capture: substituted term has free variable "${v}" that is bound inside the formula body`,
        );
      }
    }
  }

  return substituteInFormula(formula.body, boundName, term);
}

/** Walk a formula and replace var-references named `name` with `term`. */
function substituteInFormula(
  formula: IrFormula,
  name: string,
  term: IrTerm,
): IrFormula {
  switch (formula.kind) {
    case "atomic":
      return {
        kind: "atomic",
        name: formula.name,
        args: formula.args.map((a) => substituteInTerm(a, name, term)),
      };
    case "and":
    case "or":
    case "not":
    case "implies":
      return {
        kind: formula.kind,
        operands: formula.operands.map((o) => substituteInFormula(o, name, term)),
      };
    case "forall":
    case "exists": {
      // Don't shadow: if the inner binder uses the same name, stop substituting
      // beneath this binding (the inner var is a different var that happens to share the name).
      if (formula.name === name) return formula;
      return {
        kind: formula.kind,
        name: formula.name,
        sort: formula.sort,
        body: substituteInFormula(formula.body, name, term),
      };
    }
    case "choice": {
      if (formula.varName === name) return formula;
      return {
        kind: "choice",
        varName: formula.varName,
        sort: formula.sort,
        body: substituteInFormula(formula.body, name, term),
      };
    }
  }
}

/** Walk a term and replace var-references named `name` with `term`. */
function substituteInTerm(input: IrTerm, name: string, term: IrTerm): IrTerm {
  if (input.kind === "var") {
    if (input.name === name) return term;
    return input;
  }
  if (input.kind === "ctor") {
    return {
      kind: "ctor",
      name: input.name,
      args: input.args.map((a) => substituteInTerm(a, name, term)),
    };
  }
  if (input.kind === "lambda") {
    if (input.paramName === name) return input;
    return {
      kind: "lambda",
      paramName: input.paramName,
      paramSort: input.paramSort,
      body: substituteInTerm(input.body, name, term),
    };
  }
  if (input.kind === "let") {
    return {
      kind: "let",
      bindings: input.bindings.map((b) => ({
        name: b.name,
        boundTerm: substituteInTerm(b.boundTerm, name, term),
      })),
      body: substituteInTerm(input.body, name, term),
    };
  }
  return input; // const
}

function collectFreeVarNames(term: IrTerm): Set<string> {
  const out = new Set<string>();
  walkTermVars(term, (v) => out.add(v));
  return out;
}

function walkTermVars(term: IrTerm, visit: (name: string) => void): void {
  if (term.kind === "var") visit(term.name);
  else if (term.kind === "ctor") {
    for (const a of term.args) walkTermVars(a, visit);
  } else if (term.kind === "lambda") {
    walkTermVars(term.body, visit);
  } else if (term.kind === "let") {
    for (const b of term.bindings) walkTermVars(b.boundTerm, visit);
    walkTermVars(term.body, visit);
  }
}

function collectInnerBoundVarNames(formula: IrFormula): Set<string> {
  const out = new Set<string>();
  walkFormulaBindings(formula, (n) => out.add(n));
  return out;
}

function walkFormulaBindings(formula: IrFormula, visit: (name: string) => void): void {
  switch (formula.kind) {
    case "forall":
    case "exists":
      visit(formula.name);
      walkFormulaBindings(formula.body, visit);
      return;
    case "and":
    case "or":
    case "not":
    case "implies":
      formula.operands.forEach((o) => walkFormulaBindings(o, visit));
      return;
    case "atomic":
      return; // atomics introduce no bindings
    case "choice":
      visit(formula.varName);
      walkFormulaBindings(formula.body, visit);
      return;
  }
}
