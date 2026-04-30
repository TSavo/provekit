/**
 * Walks `.invariant.ts` files in a tsc.Program looking for top-level
 * `property(name, formula)` calls. Each call site produces one
 * LiftedProperty.
 *
 * Spec: docs/specs/2026-04-29-ts-ir-language.md §9 (top-level entry)
 */

import ts from "typescript";
import type { IrFormula } from "../formulas.js";
import type { LiftDiagnostic } from "./diagnostics.js";
import { makeDiagnostic } from "./diagnostics.js";
import { isPropertyCall, isAssertCall, isInvariantFile } from "./anchoring.js";
import { liftFormulaExpression, type LiftContext } from "./rules.js";
import type { PureFunctionRegistry } from "./registry.js";

export interface LiftedProperty {
  name: string;
  formula: IrFormula;
  sourceLocation: { filePath: string; line: number; column: number };
  filePath: string;
}

export function liftSourceFile(
  file: ts.SourceFile,
  checker: ts.TypeChecker,
  registry: PureFunctionRegistry,
  diagnostics: LiftDiagnostic[],
): LiftedProperty[] {
  if (!isInvariantFile(file.fileName)) return [];
  if (file.isDeclarationFile) return [];

  const results: LiftedProperty[] = [];

  function visit(node: ts.Node): void {
    if (ts.isCallExpression(node)) {
      if (isPropertyCall(node)) {
        const lifted = liftPropertyCall(node, checker, registry, diagnostics, file);
        if (lifted) results.push(lifted);
        // Don't descend into the formula args; liftFormulaExpression handled them.
        return;
      }
      if (isAssertCall(node)) {
        const lifted = liftAssertCall(node, checker, registry, diagnostics, file);
        if (lifted) results.push(lifted);
        return;
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  return results;
}

function liftPropertyCall(
  call: ts.CallExpression,
  checker: ts.TypeChecker,
  registry: PureFunctionRegistry,
  diagnostics: LiftDiagnostic[],
  file: ts.SourceFile,
): LiftedProperty | null {
  if (call.arguments.length !== 2) {
    diagnostics.push(
      makeDiagnostic(call, `property() expects exactly two arguments: (name, formula)`),
    );
    return null;
  }
  const nameArg = call.arguments[0];
  if (!ts.isStringLiteralLike(nameArg)) {
    diagnostics.push(
      makeDiagnostic(nameArg, `property()'s first argument must be a string literal`),
    );
    return null;
  }
  const name = nameArg.text;
  const formulaExpr = call.arguments[1];

  const ctx: LiftContext = { checker, diagnostics, registry, scope: [] };
  const formula = liftFormulaExpression(formulaExpr, ctx);

  const { line, character } = file.getLineAndCharacterOfPosition(call.getStart());
  return {
    name,
    formula,
    sourceLocation: {
      filePath: file.fileName,
      line: line + 1,
      column: character + 1,
    },
    filePath: file.fileName,
  };
}

function liftAssertCall(
  call: ts.CallExpression,
  checker: ts.TypeChecker,
  registry: PureFunctionRegistry,
  diagnostics: LiftDiagnostic[],
  file: ts.SourceFile,
): LiftedProperty | null {
  if (call.arguments.length !== 1) {
    diagnostics.push(
      makeDiagnostic(call, `assert() expects exactly one formula argument`),
    );
    return null;
  }
  const formulaExpr = call.arguments[0];
  const ctx: LiftContext = { checker, diagnostics, registry, scope: [] };
  const formula = liftFormulaExpression(formulaExpr, ctx);
  const { line, character } = file.getLineAndCharacterOfPosition(call.getStart());
  const name = `assert@${file.fileName}:${line + 1}:${character + 1}`;
  return {
    name,
    formula,
    sourceLocation: { filePath: file.fileName, line: line + 1, column: character + 1 },
    filePath: file.fileName,
  };
}
