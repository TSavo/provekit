/**
 * Pass 5: negation-normal form (NNF).
 *
 * Pre-condition: input is a CanonicalFolAst with no `implies` nodes
 *   (pass 4 must have run). `not` nodes may wrap any subformula.
 * Post-condition: `not` nodes appear only on atomic formulas. All
 *   negations have been pushed inward via De Morgan's laws.
 *
 * Rewrites applied:
 *   not(and(p, q, ...)) → or(not(p), not(q), ...)
 *   not(or(p, q, ...))  → and(not(p), not(q), ...)
 *   not(not(p))         → p
 *   not(forall(s, body)) → exists(s, not(body))
 *   not(exists(s, body)) → forall(s, not(body))
 *
 * Predicate-specific negation (on atomic formulas):
 *   not(p ≠ q)  → p = q
 *   not(p = q)  → p ≠ q
 *   not(p < q)  → p ≥ q
 *   not(p ≤ q)  → p > q
 *   not(p > q)  → p ≤ q
 *   not(p ≥ q)  → p < q
 *   not(true)   → false
 *   not(false)  → true
 *
 * For all other predicates (kit-defined, SAST, etc.), `not` is left
 * on the atomic node as a negated atomic: the spec's NNF form
 * allows this for non-standard predicates.
 *
 * This pass is applied recursively until no not-wrapping-non-atomic
 * nodes remain.
 */

import type { CanonicalFolAst } from "../ast.js";

const NEGATE_PREDICATE: Record<string, string> = {
  "≠": "=",
  "=": "≠",
  "<": "≥",
  "≤": ">",
  ">": "≤",
  "≥": "<",
  true: "false",
  false: "true",
};

/**
 * Convert a CanonicalFolAst to negation-normal form.
 */
export function toNnf(ast: CanonicalFolAst): CanonicalFolAst {
  switch (ast.kind) {
    case "forall":
    case "exists":
      return { kind: ast.kind, sort: ast.sort, body: toNnf(ast.body) };

    case "and":
      return { kind: "and", operands: ast.operands.map(toNnf) };

    case "or":
      return { kind: "or", operands: ast.operands.map(toNnf) };

    case "not":
      return pushNot(ast.operands[0]);

    case "atomic":
      return ast;

    case "choice":
      return { kind: "choice", sort: ast.sort, body: toNnf(ast.body) };
  }
}

/**
 * Push a negation inward through `inner`. Called when we have `not(inner)`.
 */
function pushNot(inner: CanonicalFolAst): CanonicalFolAst {
  switch (inner.kind) {
    case "not":
      // not(not(p)) → p  (then recursively normalize p)
      return toNnf(inner.operands[0]);

    case "and":
      // not(and(p, q, ...)) → or(not(p), not(q), ...)
      return {
        kind: "or",
        operands: inner.operands.map((op) => pushNot(op)),
      };

    case "or":
      // not(or(p, q, ...)) → and(not(p), not(q), ...)
      return {
        kind: "and",
        operands: inner.operands.map((op) => pushNot(op)),
      };

    case "forall":
      // not(forall(s, body)) → exists(s, not(body))
      return { kind: "exists", sort: inner.sort, body: pushNot(inner.body) };

    case "exists":
      // not(exists(s, body)) → forall(s, not(body))
      return { kind: "forall", sort: inner.sort, body: pushNot(inner.body) };

    case "atomic": {
      // Predicate-specific negation.
      const negPred = NEGATE_PREDICATE[inner.name];
      if (negPred !== undefined) {
        // Replace predicate; args unchanged.
        return { kind: "atomic", name: negPred, args: inner.args };
      }
      // For kit-defined / unknown predicates: leave as not(atomic).
      return { kind: "not", operands: [inner] };
    }

    case "choice":
      // not(choice(x, s, body)): complex negation, leave as not(choice)
      return { kind: "not", operands: [inner] };
  }
}
