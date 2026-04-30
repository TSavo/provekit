/**
 * Lift-time diagnostics. Uses tsc.Diagnostic shape so editors and CI
 * pipelines can consume them with the same machinery they already use
 * for type errors.
 *
 * Spec: docs/specs/2026-04-29-ts-ir-language.md §9 "Error reporting"
 */

import ts from "typescript";

export interface LiftDiagnostic {
  file: ts.SourceFile | undefined;
  start: number | undefined;
  length: number | undefined;
  messageText: string;
  category: ts.DiagnosticCategory;
  code: number;
  source: "provekit-lift";
}

export const LIFT_DIAGNOSTIC_CODE = 9000;

export function makeDiagnostic(
  node: ts.Node,
  message: string,
): LiftDiagnostic {
  return {
    file: node.getSourceFile(),
    start: node.getStart(),
    length: node.getWidth(),
    messageText: message,
    category: ts.DiagnosticCategory.Error,
    code: LIFT_DIAGNOSTIC_CODE,
    source: "provekit-lift",
  };
}

export function makeFileDiagnostic(
  file: ts.SourceFile,
  start: number,
  length: number,
  message: string,
): LiftDiagnostic {
  return {
    file,
    start,
    length,
    messageText: message,
    category: ts.DiagnosticCategory.Error,
    code: LIFT_DIAGNOSTIC_CODE,
    source: "provekit-lift",
  };
}

export function formatDiagnostic(d: LiftDiagnostic): string {
  if (!d.file || d.start === undefined) {
    return d.messageText;
  }
  const { line, character } = d.file.getLineAndCharacterOfPosition(d.start);
  return `${d.file.fileName}:${line + 1}:${character + 1} - ${d.messageText}`;
}
