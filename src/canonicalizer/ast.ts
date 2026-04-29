/**
 * Canonical FOL AST types.
 *
 * These are the output types of the canonicalization pipeline. Every
 * host language's IR-formula representation canonicalizes to these
 * structures; the CBOR encoding of these structures is byte-identical
 * across host languages when the formula is logically equivalent.
 *
 * Grammar (from spec docs/specs/2026-04-29-ast-canonicalizer.md):
 *
 *   CanonicalFolAst := Quantifier | Connective | Atomic
 *
 * After the full pipeline runs:
 * - No `implies` nodes remain (rewritten to or(not(a), c)).
 * - Negations appear only on Atomic nodes (NNF enforced).
 * - `and`/`or` are flattened, sorted, deduplicated, identity-removed.
 * - Bound variables are de Bruijn indices, names are erased.
 */

// ---------------------------------------------------------------------------
// Sorts
// ---------------------------------------------------------------------------

export type CanonicalSort =
  | { kind: "primitive"; name: string }
  | { kind: "set"; element: CanonicalSort }
  | { kind: "tuple"; elements: CanonicalSort[] }
  | { kind: "function"; domain: CanonicalSort[]; range: CanonicalSort };

// ---------------------------------------------------------------------------
// Terms
// ---------------------------------------------------------------------------

export type CanonicalTerm =
  | CanonicalVar
  | CanonicalConst
  | CanonicalCtor;

export type CanonicalVar = {
  kind: "var";
  index: number; // de Bruijn index
  sort: CanonicalSort;
};

export type CanonicalConst = {
  kind: "const";
  sort: CanonicalSort;
  value: boolean | number | bigint | string | null;
};

export type CanonicalCtor = {
  kind: "ctor";
  name: string;
  args: CanonicalTerm[];
  sort: CanonicalSort;
};

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

/** Standard canonical predicate names. */
export type CanonicalPredicate = string;

// ---------------------------------------------------------------------------
// AST nodes
// ---------------------------------------------------------------------------

export type CanonicalQuantifier = {
  kind: "forall" | "exists";
  sort: CanonicalSort;
  body: CanonicalFolAst;
};

/**
 * Connective node. After the full pipeline:
 * - `not` wraps only Atomic nodes.
 * - `and`/`or` have `operands` sorted, flattened, deduped.
 * - `implies` does not appear.
 */
export type CanonicalConnective =
  | { kind: "and"; operands: CanonicalFolAst[] }
  | { kind: "or"; operands: CanonicalFolAst[] }
  | { kind: "not"; body: CanonicalFolAst };

export type CanonicalAtomic = {
  kind: "atomic";
  predicate: CanonicalPredicate;
  args: CanonicalTerm[];
};

export type CanonicalFolAst =
  | CanonicalQuantifier
  | CanonicalConnective
  | CanonicalAtomic;
