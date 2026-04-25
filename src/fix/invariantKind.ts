/**
 * Shared invariant-kind classifier.
 *
 * Promoted from invariantFidelity.ts so all SMT-using oracles can route
 * uniformly. Same regex contract.
 *
 * ABSTRACT: no Int/Real declarations in the SMT body. Taint-style invariants
 * where Z3 has no canonical numerical shape, so behavioral verification
 * (oracle #9-style: regression test must fail on original / pass on fixed)
 * is the only honest discriminator.
 *
 * CONCRETE: at least one Int/Real declaration. Arithmetic-style invariants
 * (division-by-zero, off-by-one) where Z3 unsat under post-fix path
 * conditions is a real proof.
 *
 * Ground truth is the SMT body, NOT the bindings list. The formulateInvariant
 * parser defaults missing `sort` fields to "Int", so LLM-omitted sort metadata
 * cannot be trusted.
 */

import type { InvariantClaim } from "./types.js";

export type InvariantKind = "concrete" | "abstract";

/**
 * Canonical regex: matches `(declare-const NAME Int|Real)` declarations.
 * If at least one such declaration is present, the invariant is concrete.
 * Otherwise (Bool-only or no declarations), abstract.
 */
const NUMERIC_DECL_RE = /\(declare-const\s+\S+\s+(Int|Real)\b/;

export function classifyInvariantKind(invariant: InvariantClaim): InvariantKind {
  if (!NUMERIC_DECL_RE.test(invariant.formalExpression)) return "abstract";
  return "concrete";
}
