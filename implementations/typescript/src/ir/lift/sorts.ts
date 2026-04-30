/**
 * Sort resolution from TypeScript types.
 *
 * The lifter reads lambda parameter type annotations via tsc's type
 * checker. Primitive sorts are recognized by the kit's branded types
 * (Int, Real, Bool, StringSort) which carry a `__sort` property.
 * User-defined branded types follow the same shape.
 *
 * Spec: protocol/specs/2026-04-29-ts-ir-language.md §5
 */

import ts from "typescript";
import type { Sort } from "../formulas.js";

const PRIMITIVE_SORT_NAMES = new Set([
  "Bool",
  "Int",
  "Real",
  "String",
  "Ref",
  "Node",
  "Edge",
  "Region",
  "Time",
]);

export function primitiveSort(name: string): Sort {
  return { kind: "primitive", name };
}

/**
 * Resolve a TypeScript type to a Sort. Returns null if the type does
 * not carry the `__sort` brand (i.e. it's an unbranded primitive,
 * which the spec rejects). The caller emits a diagnostic in that case.
 *
 * Tuple types lift to `tuple` sorts. Array types lift to `set` sorts
 * with the element sort recursively resolved.
 */
export function resolveSort(
  type: ts.Type,
  checker: ts.TypeChecker,
): Sort | null {
  // Unwrap intersections: branded types are number & { __sort: 'X' }.
  if (type.isIntersection()) {
    for (const sub of type.types) {
      const brandName = readBrand(sub, checker);
      if (brandName !== null) {
        return primitiveSort(brandName);
      }
    }
    return null;
  }

  // Direct __sort property on the type itself
  const brandName = readBrand(type, checker);
  if (brandName !== null) {
    return primitiveSort(brandName);
  }

  // Tuple: TS encodes as TypeReference with target.objectFlags & Tuple
  if (isTupleType(type)) {
    const elementTypes = checker.getTypeArguments(type as ts.TypeReference);
    const elements: Sort[] = [];
    for (const et of elementTypes) {
      const elSort = resolveSort(et, checker);
      if (elSort === null) return null;
      elements.push(elSort);
    }
    return { kind: "tuple", elements };
  }

  // Array: encoded as TypeReference to Array<T>
  const elemType = arrayElementType(type, checker);
  if (elemType !== null) {
    const elementSort = resolveSort(elemType, checker);
    if (elementSort === null) return null;
    return { kind: "set", element: elementSort };
  }

  return null;
}

function readBrand(type: ts.Type, checker: ts.TypeChecker): string | null {
  const sym = type.getProperty("__sort");
  if (!sym) return null;
  const decl = sym.valueDeclaration ?? sym.declarations?.[0];
  if (!decl) return null;
  const t = checker.getTypeOfSymbolAtLocation(sym, decl);
  if (t.isStringLiteral()) {
    return t.value;
  }
  return null;
}

function isTupleType(type: ts.Type): boolean {
  if (!(type.flags & ts.TypeFlags.Object)) return false;
  const ot = type as ts.ObjectType;
  return (ot.objectFlags & ts.ObjectFlags.Reference) !== 0
    && ((ot as ts.TypeReference).target?.objectFlags & ts.ObjectFlags.Tuple) !== 0;
}

function arrayElementType(type: ts.Type, checker: ts.TypeChecker): ts.Type | null {
  if (!(type.flags & ts.TypeFlags.Object)) return null;
  const ot = type as ts.ObjectType;
  if ((ot.objectFlags & ts.ObjectFlags.Reference) === 0) return null;
  const tr = ot as ts.TypeReference;
  const sym = tr.target?.symbol ?? tr.symbol;
  if (!sym) return null;
  if (sym.name !== "Array" && sym.name !== "ReadonlyArray") return null;
  const args = checker.getTypeArguments(tr);
  if (args.length !== 1) return null;
  return args[0];
}

export function isPrimitiveSortName(name: string): boolean {
  return PRIMITIVE_SORT_NAMES.has(name);
}
