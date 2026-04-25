/**
 * Shared invariant-kind classifier.
 *
 * "Concrete" was originally meant to mean: Z3 can mechanically verify
 * equivalence between two formalizations of the same idea. Two LLM runs
 * formalizing `b !== 0` produce normalized SMT that Z3 can compare. Two LLM
 * runs formalizing "no duplicates in array" produce wildly different SMT
 * shapes (count-based, distinct-clause, quantifier-based) that Z3 cannot
 * align without arbitrary variable renaming.
 *
 * Binding type alone (Int vs Bool) is a proxy that breaks the moment we hit
 * set / order / cardinality semantics expressed via numerical bindings (e.g.
 * Int array indices used to assert uniqueness). Bug-1 from the BugsJS
 * hand-staging surfaced this: "no duplicate methods in Allow header" had
 * Int bindings but is fundamentally set-style.
 *
 * The classifier now uses three signals, in order:
 *   1. SMT structure hints (forall / exists / distinct / Set sort) → abstract
 *   2. Prose keywords (set / order / cardinality / uniqueness terms) → abstract
 *   3. Fallback: numeric declarations → concrete; else abstract
 *
 * If ANY abstract signal fires, route through prose-overlap + traceability
 * (the abstract path). Z3 SMT-equivalence is reserved for invariants that
 * really do have a canonical normal form.
 */

import type { InvariantClaim } from "./types.js";

export type InvariantKind = "concrete" | "abstract";

/** SMT operators / sorts that have no canonical normalization across LLM runs. */
const ABSTRACT_SMT_RE = /\(distinct\s|\(forall\s|\(exists\s|\bSet\b|\bset-union\b|\bset-member\b|\bset-intersect\b/;

/** Numeric-sorted declarations: necessary but not sufficient for concrete. */
const NUMERIC_DECL_RE = /\(declare-const\s+\S+\s+(Int|Real)\b/;

/**
 * Prose terms that signal set / order / cardinality / uniqueness semantics.
 * These topics produce many equivalent SMT formalizations across LLM runs;
 * SMT-equivalence checks fire false negatives on them. Lowercase, substring.
 */
const ABSTRACT_PROSE_KEYWORDS = [
  // Set / uniqueness
  "duplicate", "duplicates", "no two", "unique", "uniqueness",
  "distinct", "set of", "no repeat", "no overlap", "disjoint",
  // Order
  "ordered", "sorted", "monotonic", "ascending", "descending",
  "sort order",
  // Membership / containment relations
  "subset", "superset", "contains all", "contains any",
  // Cardinality (counted occurrences)
  "cardinality", "count of",
  "appears once", "appears more than", "appears at most", "appears at least",
  "exactly once", "exactly one", "exactly twice", "exactly two",
  "at most once", "at most one", "at most twice", "at most two",
  "at least once", "at least one", "at least twice", "at least two",
  "more than once", "more than one",
  "no more than", "no fewer than",
];

/**
 * Catch-all for cardinality phrasing the keyword list missed: anything that
 * mentions an "occurrence count" relationship — "occur N times", "happen
 * twice", "fire three times". These all describe set / multiplicity semantics
 * that SMT-equivalence cannot canonicalize.
 */
const CARDINALITY_RE = /\b(occurs?|occurr(ed|ing|ences?)|happens?|fires?|matches|times)\b.*\b(once|twice|thrice|\d+\s*times?)\b|\b(once|twice|thrice|\d+\s*times?)\b.*\b(occurs?|occurr(ed|ing|ences?)|happens?|fires?|matches)\b/;

export function classifyInvariantKind(invariant: InvariantClaim): InvariantKind {
  // Signal 1: SMT structure. Quantifiers, distinct-clauses, set ops.
  if (ABSTRACT_SMT_RE.test(invariant.formalExpression)) return "abstract";

  // Signal 2: prose keywords. Lowercased substring match against the
  // human-readable invariant description.
  const prose = (invariant.description ?? "").toLowerCase();
  for (const kw of ABSTRACT_PROSE_KEYWORDS) {
    if (prose.includes(kw)) return "abstract";
  }
  if (CARDINALITY_RE.test(prose)) return "abstract";

  // Signal 3: fallback. Concrete iff at least one Int/Real declaration AND
  // none of the abstract signals above fired.
  if (NUMERIC_DECL_RE.test(invariant.formalExpression)) return "concrete";
  return "abstract";
}
