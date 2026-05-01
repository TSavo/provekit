/**
 * Sort resolution from RAW TypeScript types (not branded `__sort` types).
 *
 * The existing src/ir/lift/sorts.ts requires brands because it operates
 * on `.invariant.ts` author-time text. Lift v0 operates on real source
 * code where `s: string` is plain `string`, not `String<__sort>`. This
 * module maps unbranded primitives to IR primitive sorts and refuses
 * everything else loudly.
 *
 * Spec: docs/superpowers/specs/2026-04-30-provekit-lift-v0.md, Stage 1.
 */

import ts from "typescript";
import type { Sort } from "../ir/formulas.js";

/** Primitive sort the lift adapter accepts in v0. */
export type LiftPrimitiveSort =
  | { kind: "primitive"; name: "Int" }
  | { kind: "primitive"; name: "String" }
  | { kind: "primitive"; name: "Bool" };

/**
 * Map a TS type to a primitive IR sort. Returns null when the type is
 * not one of the v0-supported primitives. Caller emits the appropriate
 * `non-primitive-surface` diagnostic with the offending type's textual
 * form.
 *
 * v0 mapping (intentionally narrow):
 *   number  -> Int     (we collapse number to Int for v0; Real is v1)
 *   string  -> String
 *   boolean -> Bool
 *
 * Anything else - arrays, tuples, generics, unions, intersections,
 * objects, interfaces, type literals - returns null. Lift refuses.
 */
export function detectPrimitiveSort(
  type: ts.Type,
  checker: ts.TypeChecker,
): LiftPrimitiveSort | null {
  // Reject unions/intersections eagerly; v0 is single-type per arg.
  if (type.isUnion() || type.isIntersection()) return null;

  const flags = type.getFlags();

  // Plain primitives exposed by the type system as flag bits.
  if (flags & ts.TypeFlags.Number) return { kind: "primitive", name: "Int" };
  if (flags & ts.TypeFlags.String) return { kind: "primitive", name: "String" };
  if (flags & ts.TypeFlags.Boolean) return { kind: "primitive", name: "Bool" };
  if (flags & ts.TypeFlags.BooleanLiteral) return { kind: "primitive", name: "Bool" };

  // Numeric / string literal types: TS sometimes reports `42` as
  // NumberLiteral. Treat as Int / String for v0; subtypes of primitives
  // still satisfy the surface contract.
  if (flags & ts.TypeFlags.NumberLiteral) return { kind: "primitive", name: "Int" };
  if (flags & ts.TypeFlags.StringLiteral) return { kind: "primitive", name: "String" };

  // Quiet on `void`, `null`, `undefined`, `any`, `unknown`, `never`,
  // object types, generics. The caller treats null as "refuse loudly."
  void checker;
  return null;
}

/** Convenience helper: textual rendering of a TS type for diagnostics. */
export function typeToDiagnosticString(
  type: ts.Type,
  checker: ts.TypeChecker,
): string {
  return checker.typeToString(type);
}
