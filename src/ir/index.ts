/**
 * @provekit/ir — TypeScript reference IR library.
 *
 * Two surfaces:
 * - Type-dialect: branded types (NonZero, NonEmpty, …) verified by tsserver.
 * - Library-dialect: value-level IR formulas (forAll, exists, and, …)
 *   that evaluate to IrFormula data structures for downstream tooling.
 *
 * Zero runtime dependencies.
 */

// ---------------------------------------------------------------------------
// Types (formulas, sorts, bindings)
// ---------------------------------------------------------------------------

export type {
  IrFormula,
  IrFormulaLambda,
  IrTerm,
  Sort,
  AtomicPredicate,
  BindingScope,
  Bindings,
  CompilationHint,
} from "./formulas.js";

export { liftToTerm } from "./formulas.js";

// ---------------------------------------------------------------------------
// Sorts (runtime values)
// ---------------------------------------------------------------------------

export {
  Bool,
  Int,
  Real,
  String,
  Ref,
  Node,
  Edge,
  SetOf,
  TupleOf,
  FuncOf,
} from "./sorts.js";

// ---------------------------------------------------------------------------
// Type-dialect brands
// ---------------------------------------------------------------------------

export type {
  Branded,
  NonZero,
  NonEmpty,
  Sorted,
  NonNull,
  Validated,
  Refined,
  Range,
} from "./brands.js";

export {
  nonZero,
  assertNonZero,
  nonEmpty,
  assertNonEmpty,
  sorted,
  assertSorted,
  nonNull,
  assertNonNull,
  refined,
  range,
  assertRange,
} from "./brands.js";

// ---------------------------------------------------------------------------
// Quantifiers
// ---------------------------------------------------------------------------

export { forAll, exists, forSome } from "./quantifiers.js";

// ---------------------------------------------------------------------------
// Connectives
// ---------------------------------------------------------------------------

export { and, or, not, implies, iff } from "./connectives.js";

// ---------------------------------------------------------------------------
// Assertion namespace
// ---------------------------------------------------------------------------

export { assert } from "./assert.js";

// ---------------------------------------------------------------------------
// Scope helpers
// ---------------------------------------------------------------------------

export {
  function_,
  module_,
  class_,
  method_,
  region,
  transition,
  whenever,
} from "./scopes.js";

// ---------------------------------------------------------------------------
// Property constructor
// ---------------------------------------------------------------------------

export type { Property } from "./property.js";
export { property } from "./property.js";
