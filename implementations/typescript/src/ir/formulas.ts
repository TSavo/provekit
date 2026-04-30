/**
 * Internal IR formula data structures. The AST canonicalizer
 * (src/canonicalizer/) consumes these directly via re-export;
 * everything else in the IR library produces them.
 *
 * This file is the single source of truth for the IR-formula shape.
 * Zero runtime dependencies — pure type and helper definitions.
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
  | { kind: "bitvec"; width: number }
  | { kind: "set"; element: Sort }
  | { kind: "tuple"; elements: Sort[] }
  | { kind: "function"; domain: Sort[]; range: Sort };

// ---------------------------------------------------------------------------
// Terms
//
// VarTerm and CtorTerm carry NO sort. ConstTerm keeps sort because the
// literal's type is not derivable from binding scope.
// ---------------------------------------------------------------------------

export type VarTerm = { kind: "var"; name: string };
export type ConstTerm = { kind: "const"; value: unknown; sort: Sort };
export type CtorTerm = { kind: "ctor"; name: string; args: IrTerm[] };

export type IrTerm = VarTerm | ConstTerm | CtorTerm;

// ---------------------------------------------------------------------------
// Atomic predicates
// ---------------------------------------------------------------------------

export type AtomicPredicate =
  | "=" | "≠" | "<" | "≤" | ">" | "≥"
  | "true" | "false"
  | "subset" | "member"
  | "kind-of" | "data-flows-to" | "dominates" | "post-dominates"
  | "transition-from-to"
  | "on-path"
  // SMT-LIB BV comparison predicates (return Bool).
  | "bvult" | "bvule" | "bvugt" | "bvuge"
  | "bvslt" | "bvsle" | "bvsgt" | "bvsge"
  | string; // kit-defined extensions

// ---------------------------------------------------------------------------
// Formulas
//
// Maximal-uniformity flat shape: every quantifier is { kind, name, sort, body },
// every connective is { kind, operands }, every atomic is { kind, name, args }.
// No nested Lambda wrapper, no kind-specific operand field names.
// ---------------------------------------------------------------------------

export type QuantifierFormula = {
  kind: "forall" | "exists";
  name: string;
  sort: Sort;
  body: IrFormula;
};

export type ConnectiveFormula = {
  kind: "and" | "or" | "not" | "implies";
  operands: IrFormula[];
};

export type AtomicFormula = {
  kind: "atomic";
  name: AtomicPredicate;
  args: IrTerm[];
};

export type IrFormula = QuantifierFormula | ConnectiveFormula | AtomicFormula;

// ---------------------------------------------------------------------------
// Binding scope (for property declarations)
// ---------------------------------------------------------------------------

export type BindingScope =
  | { kind: "function"; name: string }
  | { kind: "module"; path: string }
  | { kind: "class"; name: string }
  | { kind: "method"; className: string; methodName: string }
  | { kind: "region"; start: string; end: string }
  | { kind: "transition"; name: string }
  | { kind: "whenever"; predicate: IrFormula };

// ---------------------------------------------------------------------------
// Compilation hint
// ---------------------------------------------------------------------------

export type CompilationHint =
  | "datalog-friendly"
  | "requires-smt"
  | "behavioral"
  | "auto"
  | string; // kit extensions

// ---------------------------------------------------------------------------
// Bindings map (sort values keyed by binding name)
// ---------------------------------------------------------------------------

export type Bindings = Record<string, Sort>;

// ---------------------------------------------------------------------------
// Helper: lift a raw JS value to an IrTerm (used by assert builders)
// ---------------------------------------------------------------------------

/**
 * Lift a value that is either already an IrTerm or a primitive JS value
 * into an IrTerm. Primitive numbers map to Int const, strings to String const,
 * booleans to Bool const. If the value is already an IrTerm (has a `kind`
 * field matching term kinds), it is returned as-is.
 */
export function liftToTerm(value: IrTerm | number | bigint | string | boolean | null): IrTerm {
  if (value !== null && typeof value === "object" && "kind" in value) {
    const k = (value as IrTerm).kind;
    if (k === "var" || k === "const" || k === "ctor") {
      return value as IrTerm;
    }
  }
  if (typeof value === "number") {
    return { kind: "const", value, sort: { kind: "primitive", name: "Int" } };
  }
  if (typeof value === "bigint") {
    return { kind: "const", value, sort: { kind: "primitive", name: "Int" } };
  }
  if (typeof value === "string") {
    return { kind: "const", value, sort: { kind: "primitive", name: "String" } };
  }
  if (typeof value === "boolean") {
    return { kind: "const", value, sort: { kind: "primitive", name: "Bool" } };
  }
  // null -> Ref const
  return { kind: "const", value: null, sort: { kind: "primitive", name: "Ref" } };
}
