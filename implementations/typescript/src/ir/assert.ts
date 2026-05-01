/**
 * Assertion namespace. Each function constructs an atomic IrFormula.
 *
 * Arguments can be IrTerm objects or primitive JS values. Primitives
 * are lifted to `const` terms via `liftToTerm`.
 */

import type { IrFormula, IrTerm } from "./formulas.js";
import { liftToTerm } from "./formulas.js";

type Liftable = IrTerm | number | bigint | string | boolean | null;

function lift(v: Liftable): IrTerm {
  return liftToTerm(v);
}

function atom(predicateName: string, args: Liftable[]): IrFormula {
  return { kind: "atomic", name: predicateName, args: args.map(lift) };
}

// ---------------------------------------------------------------------------
// Comparison assertions
// ---------------------------------------------------------------------------

/** Assert that a = b. */
function equal(a: Liftable, b: Liftable): IrFormula {
  return atom("=", [a, b]);
}

/** Assert that a ≠ b. */
function notEqual(a: Liftable, b: Liftable): IrFormula {
  return atom("≠", [a, b]);
}

/** Assert that a < b. */
function lessThan(a: Liftable, b: Liftable): IrFormula {
  return atom("<", [a, b]);
}

/** Assert that a ≤ b. */
function lessThanOrEqual(a: Liftable, b: Liftable): IrFormula {
  return atom("≤", [a, b]);
}

/** Assert that a > b. */
function greaterThan(a: Liftable, b: Liftable): IrFormula {
  return atom(">", [a, b]);
}

/** Assert that a ≥ b. */
function greaterThanOrEqual(a: Liftable, b: Liftable): IrFormula {
  return atom("≥", [a, b]);
}

// ---------------------------------------------------------------------------
// Boolean assertions
// ---------------------------------------------------------------------------

/** Assert that b is true. */
// Named `true_` internally to avoid shadowing the global `true` keyword,
// but exposed on the namespace as `.true`.
function true_(b: Liftable): IrFormula {
  return atom("true", [b]);
}

/** Assert that b is false. */
function false_(b: Liftable): IrFormula {
  return atom("false", [b]);
}

// ---------------------------------------------------------------------------
// Set assertions
// ---------------------------------------------------------------------------

/** Assert that a ⊆ b. */
function subset(a: Liftable, b: Liftable): IrFormula {
  return atom("subset", [a, b]);
}

/** Assert that x ∈ set. */
function member(x: Liftable, set: Liftable): IrFormula {
  return atom("member", [x, set]);
}

// ---------------------------------------------------------------------------
// SAST / graph predicates
// ---------------------------------------------------------------------------

/** Assert that node has the given kind label. */
function kindOf(node: Liftable, kind: string): IrFormula {
  return atom("kind-of", [node, kind]);
}

/** Assert that data flows from a to b. */
function dataFlowsTo(a: Liftable, b: Liftable): IrFormula {
  return atom("data-flows-to", [a, b]);
}

/** Assert that a dominates b in the CFG. */
function dominates(a: Liftable, b: Liftable): IrFormula {
  return atom("dominates", [a, b]);
}

/** Assert that intermediary is on the path from source to sink. */
function onPath(intermediary: Liftable, source: Liftable, sink: Liftable): IrFormula {
  return atom("on-path", [intermediary, source, sink]);
}

// ---------------------------------------------------------------------------
// Temporal predicate (two-step builder)
// ---------------------------------------------------------------------------

/**
 * Build a `transition-from-to` atomic formula.
 *
 * Usage: `Assert.transitionFrom(pre).to(post)`
 */
function transitionFrom(pre: Liftable): { to: (post: Liftable) => IrFormula } {
  return {
    to(post: Liftable): IrFormula {
      return atom("transition-from-to", [pre, post]);
    },
  };
}

// ---------------------------------------------------------------------------
// The exported namespace
// ---------------------------------------------------------------------------

export const assert = {
  equal,
  notEqual,
  lessThan,
  lessThanOrEqual,
  greaterThan,
  greaterThanOrEqual,
  /** Assert that b is true. */
  true: true_,
  /** Assert that b is false. */
  false: false_,
  subset,
  member,
  kindOf,
  dataFlowsTo,
  dominates,
  transitionFrom,
  onPath,
};
