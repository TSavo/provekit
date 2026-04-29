/**
 * IrFormula — input shape consumed by the AST canonicalizer.
 *
 * These types mirror the `IrFormula` data structure defined in the IR
 * library spec (docs/specs/2026-04-29-ir-library.md §"Internal
 * representation"). They are defined here independently so that the
 * canonicalizer does not depend on the IR library package (which is
 * being implemented in parallel). Type-compatibility with the IR
 * library's exported types is deferred to a separate alignment pass.
 *
 * The canonical authoring form that produces these values looks like:
 *   forAll(b => assert.notEqual(b, 0))
 * which evaluates to:
 *   { kind: "forall", sort: IntSort, predicate: { kind: "lambda",
 *     varName: "b", sort: IntSort,
 *     body: { kind: "atomic", predicate: "≠",
 *             args: [{ kind: "var", name: "b", sort: IntSort },
 *                    { kind: "const", value: 0, sort: IntSort }] } } }
 */

// ---------------------------------------------------------------------------
// Sorts
// ---------------------------------------------------------------------------

export type PrimitiveSortName =
  | "Bool"
  | "Int"
  | "Real"
  | "String"
  | "Ref"
  | "Node"
  | "Edge"
  | "Region"
  | "Time";

export type Sort =
  | { kind: "primitive"; name: PrimitiveSortName | string }
  | { kind: "set"; element: Sort }
  | { kind: "tuple"; elements: Sort[] }
  | { kind: "function"; domain: Sort[]; range: Sort };

// ---------------------------------------------------------------------------
// Terms
// ---------------------------------------------------------------------------

export type IrTerm =
  | { kind: "var"; name: string; sort: Sort }
  | { kind: "const"; value: unknown; sort: Sort }
  | { kind: "ctor"; name: string; args: IrTerm[]; sort: Sort };

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

/**
 * Standard predicate names. Kit-defined predicates use a
 * "<kit-name>:<cid>" prefix and are passed through as-is.
 */
export type AtomicPredicate =
  | "="
  | "≠"
  | "<"
  | "≤"
  | ">"
  | "≥"
  | "true"
  | "false"
  | "member"
  | "subset"
  | "kind-of"
  | "data-flows-to"
  | "dominates"
  | "post-dominates"
  | "on-path"
  | "transition-from-to"
  | string; // kit-defined extensions

// ---------------------------------------------------------------------------
// Formulas
// ---------------------------------------------------------------------------

/**
 * A lambda binder used by quantifiers. The canonicalizer erases
 * `varName` and replaces var references in `body` with de Bruijn
 * indices. The `sort` becomes the quantifier node's sort.
 */
export type IrFormulaLambda = {
  kind: "lambda";
  varName: string;
  sort: Sort;
  body: IrFormula;
};

export type IrFormula =
  | { kind: "forall"; sort: Sort; predicate: IrFormulaLambda }
  | { kind: "exists"; sort: Sort; predicate: IrFormulaLambda }
  | { kind: "and"; conjuncts: IrFormula[] }
  | { kind: "or"; disjuncts: IrFormula[] }
  | { kind: "not"; body: IrFormula }
  | { kind: "implies"; antecedent: IrFormula; consequent: IrFormula }
  | { kind: "atomic"; predicate: AtomicPredicate; args: IrTerm[] };
