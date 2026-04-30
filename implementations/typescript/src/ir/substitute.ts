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
 * Today's IR uses NAMED bound variables (varName field). Substitution
 * by name match is correct provided the substituted term has no free
 * variables that collide with names bound deeper in the formula. For
 * the v1 use case (string literals, integer literals, other Const
 * terms with no free variables), this is automatically safe. A more
 * general substitution that handles variable-substitution-into-a-
 * variable-binding would require alpha-renaming; deferred.
 */

import type { IrFormula, IrTerm, IrFormulaLambda } from "./formulas.js";

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
  const lambda = formula.predicate;
  const boundName = lambda.varName;

  // Capture safety: term must not contain free vars whose names are
  // bound by lambdas inside the body. v1 callers pass Const terms only,
  // so this is automatically safe; we still check.
  const termFreeVars = collectFreeVarNames(term);
  if (termFreeVars.size > 0) {
    const innerBindings = collectInnerBoundVarNames(lambda.body);
    for (const v of termFreeVars) {
      if (innerBindings.has(v)) {
        throw new SubstituteError(
          `capture: substituted term has free variable "${v}" that is bound inside the formula body`,
        );
      }
    }
  }

  return substituteInFormula(lambda.body, boundName, term);
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
        predicate: formula.predicate,
        args: formula.args.map((a) => substituteInTerm(a, name, term)),
      };
    case "and":
      return {
        kind: "and",
        conjuncts: formula.conjuncts.map((c) => substituteInFormula(c, name, term)),
      };
    case "or":
      return {
        kind: "or",
        disjuncts: formula.disjuncts.map((d) => substituteInFormula(d, name, term)),
      };
    case "not":
      return { kind: "not", body: substituteInFormula(formula.body, name, term) };
    case "implies":
      return {
        kind: "implies",
        antecedent: substituteInFormula(formula.antecedent, name, term),
        consequent: substituteInFormula(formula.consequent, name, term),
      };
    case "forall":
    case "exists": {
      // Don't shadow: if the inner lambda binds the same name, stop substituting
      // beneath this binding (the inner var is a different var that happens to share the name).
      if (formula.predicate.varName === name) return formula;
      const innerLambda: IrFormulaLambda = {
        kind: "lambda",
        varName: formula.predicate.varName,
        sort: formula.predicate.sort,
        body: substituteInFormula(formula.predicate.body, name, term),
      };
      return { kind: formula.kind, sort: formula.sort, predicate: innerLambda };
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
      sort: input.sort,
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
      visit(formula.predicate.varName);
      walkFormulaBindings(formula.predicate.body, visit);
      return;
    case "and":
      formula.conjuncts.forEach((c) => walkFormulaBindings(c, visit));
      return;
    case "or":
      formula.disjuncts.forEach((d) => walkFormulaBindings(d, visit));
      return;
    case "not":
      walkFormulaBindings(formula.body, visit);
      return;
    case "implies":
      walkFormulaBindings(formula.antecedent, visit);
      walkFormulaBindings(formula.consequent, visit);
      return;
    case "atomic":
      return; // atomics introduce no bindings
  }
}
