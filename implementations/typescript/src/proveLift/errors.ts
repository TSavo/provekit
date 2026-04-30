/**
 * Diagnostic shapes for the prove-lift pipeline.
 *
 * Every failure mode in v0 surfaces as a typed diagnostic with a stable
 * `code` string. The CLI maps codes to exit codes per the design doc
 * at docs/superpowers/specs/2026-04-30-provekit-lift-v0.md. Diagnostics
 * are never silent: even an empty surviving-candidate set emits a
 * diagnostic before returning.
 */

export type LiftDiagnosticCode =
  | "non-primitive-surface"
  | "multiple-exports"
  | "no-exports"
  | "no-tests-cover-this-surface"
  | "all-candidates-dropped"
  | "llm-unavailable"
  | "signing-key-missing"
  | "unsupported-export-shape";

export interface LiftDiagnostic {
  code: LiftDiagnosticCode;
  filePath: string;
  /** 1-based line number into filePath. 0 means "whole file." */
  line: number;
  /** Human-readable message. Stable enough to assert against in tests. */
  message: string;
  /** Optional secondary details (e.g. the offending TS type's textual form). */
  detail?: string;
}

export class LiftError extends Error {
  constructor(public readonly diagnostic: LiftDiagnostic) {
    super(`${diagnostic.code} at ${diagnostic.filePath}:${diagnostic.line}: ${diagnostic.message}`);
    this.name = "LiftError";
  }
}

export function makeDiagnostic(
  code: LiftDiagnosticCode,
  filePath: string,
  line: number,
  message: string,
  detail?: string,
): LiftDiagnostic {
  return detail !== undefined
    ? { code, filePath, line, message, detail }
    : { code, filePath, line, message };
}
