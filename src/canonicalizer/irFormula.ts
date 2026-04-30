/**
 * IrFormula — input shape consumed by the AST canonicalizer.
 *
 * Re-exported from the IR library (`src/ir/formulas.ts`) so the
 * canonicalizer's pipeline accepts what the IR library emits without
 * any translation step. This module exists as a stable import path
 * for canonicalizer internals; the canonical type definitions live
 * in the IR library.
 *
 * Spec: protocol/specs/2026-04-29-ir-library.md §"Internal representation"
 *       protocol/specs/2026-04-29-ast-canonicalizer.md §"The canonical FOL AST"
 */

export type {
  Sort,
  PrimitiveSortName,
  IrTerm,
  AtomicPredicate,
  IrFormulaLambda,
  IrFormula,
} from "../ir/formulas.js";
