/**
 * Public API for the TS-IR lifter.
 *
 * The lifter projects type-checked TypeScript expressions in
 * `.invariant.ts` files into canonical IrFormula values that the
 * canonicalizer (src/canonicalizer/) consumes verbatim.
 *
 * Spec: docs/specs/2026-04-29-ts-ir-language.md
 *
 * Entry points:
 *   liftProject(program)         - lift every .invariant.ts in the program
 *   liftFile(filePath, program)  - lift one file
 *   liftFormulaExpression(...)   - lift a bare expression (testing)
 */

import ts from "typescript";
import type { LiftDiagnostic } from "./diagnostics.js";
import { liftSourceFile, type LiftedProperty } from "./visitor.js";
import { checkAnchoring, isInvariantFile } from "./anchoring.js";
import {
  defaultTsKitRegistry,
  type PureFunctionRegistry,
} from "./registry.js";
import {
  liftFormulaExpression,
  liftTermExpression,
  type LiftContext,
} from "./rules.js";

export type { LiftedProperty } from "./visitor.js";
export type { LiftDiagnostic } from "./diagnostics.js";
export { formatDiagnostic } from "./diagnostics.js";
export type { LiftContext } from "./rules.js";
export {
  defaultTsKitRegistry,
  emptyRegistry,
  extendRegistry,
  type PureFunctionRegistry,
  type RegistryEntry,
} from "./registry.js";
export { resolveSort } from "./sorts.js";
export { checkAnchoring, isInvariantFile } from "./anchoring.js";
export { liftFormulaExpression, liftTermExpression };

export interface LiftResult {
  properties: LiftedProperty[];
  diagnostics: LiftDiagnostic[];
}

export interface LiftProjectOptions {
  registry?: PureFunctionRegistry;
}

export function liftProject(
  program: ts.Program,
  options: LiftProjectOptions = {},
): LiftResult {
  const checker = program.getTypeChecker();
  const registry = options.registry ?? defaultTsKitRegistry();
  const properties: LiftedProperty[] = [];
  const diagnostics: LiftDiagnostic[] = [];

  for (const file of program.getSourceFiles()) {
    if (file.isDeclarationFile) continue;
    if (isInvariantFile(file.fileName)) {
      properties.push(...liftSourceFile(file, checker, registry, diagnostics));
    } else {
      diagnostics.push(...checkAnchoring(file));
    }
  }

  return { properties, diagnostics };
}

export function liftFile(
  filePath: string,
  program: ts.Program,
  options: LiftProjectOptions = {},
): LiftedProperty[] {
  const file = program.getSourceFile(filePath);
  if (!file) {
    throw new Error(`No source file found at ${filePath}`);
  }
  const checker = program.getTypeChecker();
  const registry = options.registry ?? defaultTsKitRegistry();
  const diagnostics: LiftDiagnostic[] = [];
  return liftSourceFile(file, checker, registry, diagnostics);
}
