/**
 * File anchoring enforcement: provekit.property may only appear in
 * files whose path ends in `.invariant.ts`. Production code must
 * remain bit-identical to its pre-ProvekIt state.
 *
 * Spec: protocol/specs/2026-04-29-ts-ir-language.md §3
 */

import ts from "typescript";
import type { LiftDiagnostic } from "./diagnostics.js";
import { makeDiagnostic } from "./diagnostics.js";

const ANCHORING_MESSAGE =
  "provekit.property may only appear in .invariant.ts files. " +
  "Move this declaration to a co-located .invariant.ts file.";

export function isInvariantFile(path: string): boolean {
  return path.endsWith(".invariant.ts");
}

/**
 * Walk a SourceFile looking for calls to `property(...)` (whether
 * imported as `property`, destructured from `provekit/ir`, or
 * accessed via a namespace import like `provekit.property`).
 *
 * If the file is NOT an invariant file, every such call is a
 * diagnostic. Returns an empty array for invariant files (the
 * visitor pass handles them) or for files with no property calls.
 */
export function checkAnchoring(file: ts.SourceFile): LiftDiagnostic[] {
  if (isInvariantFile(file.fileName)) return [];
  if (file.isDeclarationFile) return [];

  const diagnostics: LiftDiagnostic[] = [];

  function visit(node: ts.Node): void {
    if (ts.isCallExpression(node) && isPropertyCall(node)) {
      diagnostics.push(makeDiagnostic(node, ANCHORING_MESSAGE));
    }
    ts.forEachChild(node, visit);
  }
  visit(file);

  return diagnostics;
}

export function isPropertyCall(call: ts.CallExpression): boolean {
  const callee = call.expression;
  if (ts.isIdentifier(callee)) {
    return callee.text === "property";
  }
  if (ts.isPropertyAccessExpression(callee)) {
    return callee.name.text === "property";
  }
  return false;
}

export function isAssertCall(call: ts.CallExpression): boolean {
  const callee = call.expression;
  if (ts.isIdentifier(callee)) {
    return callee.text === "assert";
  }
  if (ts.isPropertyAccessExpression(callee)) {
    return callee.name.text === "assert";
  }
  return false;
}
