/**
 * Extension authoring API — the user-facing factory functions for
 * declaring new sorts, predicates, and ctors as extensions.
 *
 * The DX promise: authoring an extension is one function call. The
 * factory registers the declaration in the kit's registry, and returns
 * a value (a Sort, a predicate function, or a ctor function) that the
 * user can use directly in IR formulas. The registry is consulted by
 * verifiers; the returned value is consumed by IR builders.
 *
 *   const FixedPoint8 = extensionSort({
 *     name: "FixedPoint8",
 *     semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
 *     compilers: ["smt-lib"],
 *   });
 *
 *   const fixedPointMul = extensionCtor({
 *     name: "fixed-point-mul",
 *     argSorts: [FixedPoint8, FixedPoint8],
 *     returnSort: FixedPoint8,
 *     semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
 *     compilers: ["smt-lib"],
 *   });
 *
 *   // User writes invariants the same way they would with built-ins:
 *   property("commutative",
 *     forAll(FixedPoint8, x => forAll(FixedPoint8, y =>
 *       eq(fixedPointMul(x, y), fixedPointMul(y, x)))));
 *
 * The kit's own "built-in" primitives (parseInt, abs, floor, ...) are
 * authored with these same factories at module load. There's no
 * two-tier "built-in vs extension" distinction; everything is an
 * extension. Some extensions ship with the kit; others are added by
 * users at runtime. Same machinery either way.
 */

import type { IrFormula, IrTerm, Sort } from "../formulas.js";
import { liftToTerm } from "../formulas.js";
import {
  registerExtensionDeclaration,
  type SemanticDeclaration,
  type SortExtensionDeclaration,
  type PredicateExtensionDeclaration,
  type CtorExtensionDeclaration,
  type SortRef,
} from "./registry.js";

// ---------------------------------------------------------------------------
// extensionSort — declare a new sort
// ---------------------------------------------------------------------------

export interface ExtensionSortInput {
  name: string;
  /** Sort parameters (e.g. `BitVec[N]` takes a width). Default: empty. */
  params?: Array<{ name: string; paramSort: "Int" | "Bool" | "String" }>;
  semantics: SemanticDeclaration[];
  compilers: string[];
  /** CIDs of other extension declarations this depends on. */
  dependsOn?: string[];
}

/**
 * Declare a new sort and return its Sort value. Registers the extension
 * declaration in the kit's registry. Idempotent for byte-identical
 * re-registration; throws ExtensionRegistrationError on collision.
 */
export function extensionSort(input: ExtensionSortInput): Sort {
  const decl: SortExtensionDeclaration = {
    introduces: "sort",
    name: input.name,
    ...(input.params ? { params: input.params } : {}),
    semantics: input.semantics,
    compilers: input.compilers,
    ...(input.dependsOn ? { dependsOn: input.dependsOn } : {}),
  };
  registerExtensionDeclaration(decl);
  return { kind: "primitive", name: input.name };
}

// ---------------------------------------------------------------------------
// extensionPredicate — declare a new atomic predicate
// ---------------------------------------------------------------------------

export interface ExtensionPredicateInput {
  name: string;
  argSorts: SortRef[];
  semantics: SemanticDeclaration[];
  compilers: string[];
  dependsOn?: string[];
}

/**
 * Declare a new predicate and return a function that builds atomic
 * IrFormula nodes referencing it. Variadic on the predicate's arity:
 * a binary predicate's returned function takes two IrTerm args.
 */
export function extensionPredicate(
  input: ExtensionPredicateInput,
): (...args: IrTerm[]) => IrFormula {
  const decl: PredicateExtensionDeclaration = {
    introduces: "predicate",
    name: input.name,
    argSorts: input.argSorts,
    semantics: input.semantics,
    compilers: input.compilers,
    ...(input.dependsOn ? { dependsOn: input.dependsOn } : {}),
  };
  registerExtensionDeclaration(decl);
  return (...args: IrTerm[]): IrFormula => ({
    kind: "atomic",
    predicate: input.name,
    args,
  });
}

// ---------------------------------------------------------------------------
// extensionCtor — declare a new term constructor
// ---------------------------------------------------------------------------

export interface ExtensionCtorInput {
  name: string;
  argSorts: SortRef[];
  returnSort: SortRef;
  semantics: SemanticDeclaration[];
  compilers: string[];
  dependsOn?: string[];
}

/**
 * Declare a new term constructor and return a function that builds
 * IrTerm ctor nodes. Variadic on arity. The return sort is captured at
 * declaration time and applied to every emitted IrTerm.
 */
export function extensionCtor(
  input: ExtensionCtorInput,
): (...args: Array<IrTerm | number | bigint | string | boolean>) => IrTerm {
  const decl: CtorExtensionDeclaration = {
    introduces: "ctor",
    name: input.name,
    argSorts: input.argSorts,
    returnSort: input.returnSort,
    semantics: input.semantics,
    compilers: input.compilers,
    ...(input.dependsOn ? { dependsOn: input.dependsOn } : {}),
  };
  registerExtensionDeclaration(decl);
  const returnSort = resolveSort(input.returnSort);
  return (...args): IrTerm => ({
    kind: "ctor",
    name: input.name,
    args: args.map((a) => liftToTerm(a as IrTerm | number | bigint | string | boolean)),
    sort: returnSort,
  });
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function resolveSort(ref: SortRef): Sort {
  if (typeof ref === "string") {
    return { kind: "primitive", name: ref };
  }
  return ref;
}
