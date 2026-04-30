/**
 * IntentReport contract: the canonical artifact every Part B stage produces
 * and consumes. The spec at protocol/specs/2026-04-27-constraint-driven-development.md
 * (§ "The intent report" + § "Intent report schema") names this surface as the
 * stage-to-stage contract; this module is the runtime form.
 *
 * Source of truth for the type shapes is src/fix/intake/retrospective.ts —
 * those types are re-exported here so callers can import the contract from
 * one place without coupling to the retrospective module's other exports.
 *
 * The validateIntentReport() gate is hand-rolled (no zod dependency) because
 * the package's runtime cost matters and the schema is small. It throws an
 * IntentReportValidationError on the first violation, with a path-style field
 * locator so the failing site is obvious in logs.
 */

import type {
  IntentReport,
  IntentReportIntent,
  IntentReportConstraintCandidate,
  IntentReportOutputBundle,
  IntentReportTrigger,
  IntentReportCitation,
} from "../fix/intake/retrospective.js";

export type {
  IntentReport,
  IntentReportIntent,
  IntentReportConstraintCandidate,
  IntentReportOutputBundle,
  IntentReportTrigger,
  IntentReportCitation,
};

/**
 * Thrown when an IntentReport fails contract validation. `path` is a
 * dot-delimited locator of the failing field (e.g., "intents[2].lineRange[1]")
 * so callers and logs can navigate to the violation directly.
 */
export class IntentReportValidationError extends Error {
  constructor(
    public readonly path: string,
    public readonly reason: string,
    public readonly received: unknown,
  ) {
    super(`IntentReport validation failed at ${path}: ${reason}`);
  }
}

/**
 * Validate an IntentReport against the contract. Returns the report typed-narrowed
 * on success; throws IntentReportValidationError on the first violation.
 *
 * Intent: stages call this on the report they're about to emit (validates own
 * output) and on the report they receive from upstream (validates input).
 * Belt-and-suspenders: a corrupt report cannot silently flow downstream.
 */
export function validateIntentReport(value: unknown): IntentReport {
  if (!isPlainObject(value)) {
    throw new IntentReportValidationError("$", "expected object", value);
  }

  const v = value as Record<string, unknown>;

  if (v.source !== "prospective" && v.source !== "retrospective") {
    throw new IntentReportValidationError(
      "source",
      `expected "prospective" | "retrospective", got ${quote(v.source)}`,
      v.source,
    );
  }

  validateTrigger(v.trigger, "trigger");

  if (!Array.isArray(v.intents)) {
    throw new IntentReportValidationError("intents", "expected array", v.intents);
  }
  for (let i = 0; i < v.intents.length; i++) {
    validateIntent(v.intents[i], `intents[${i}]`);
  }

  validateOutputBundle(v.outputBundle, "outputBundle");

  return value as unknown as IntentReport;
}

function validateTrigger(value: unknown, path: string): void {
  if (!isPlainObject(value)) {
    throw new IntentReportValidationError(path, "expected object", value);
  }
  const t = value as Record<string, unknown>;

  if (t.kind !== "problem_statement" && t.kind !== "commit") {
    throw new IntentReportValidationError(
      `${path}.kind`,
      `expected "problem_statement" | "commit", got ${quote(t.kind)}`,
      t.kind,
    );
  }
  if (typeof t.ref !== "string" || t.ref.length === 0) {
    throw new IntentReportValidationError(
      `${path}.ref`,
      "expected non-empty string",
      t.ref,
    );
  }
  if (t.diff !== undefined && typeof t.diff !== "string") {
    throw new IntentReportValidationError(
      `${path}.diff`,
      "expected string when present",
      t.diff,
    );
  }
  if (t.commitMessage !== undefined && typeof t.commitMessage !== "string") {
    throw new IntentReportValidationError(
      `${path}.commitMessage`,
      "expected string when present",
      t.commitMessage,
    );
  }
}

function validateIntent(value: unknown, path: string): void {
  if (!isPlainObject(value)) {
    throw new IntentReportValidationError(path, "expected object", value);
  }
  const i = value as Record<string, unknown>;

  if (typeof i.filePath !== "string" || i.filePath.length === 0) {
    throw new IntentReportValidationError(
      `${path}.filePath`,
      "expected non-empty string",
      i.filePath,
    );
  }

  if (
    !Array.isArray(i.lineRange) ||
    i.lineRange.length !== 2 ||
    typeof i.lineRange[0] !== "number" ||
    typeof i.lineRange[1] !== "number"
  ) {
    throw new IntentReportValidationError(
      `${path}.lineRange`,
      "expected [number, number]",
      i.lineRange,
    );
  }
  const [start, end] = i.lineRange as [number, number];
  if (!Number.isInteger(start) || !Number.isInteger(end)) {
    throw new IntentReportValidationError(
      `${path}.lineRange`,
      "expected integer line numbers",
      i.lineRange,
    );
  }
  if (start < 1) {
    throw new IntentReportValidationError(
      `${path}.lineRange[0]`,
      "lineRange is 1-indexed; start must be >= 1",
      start,
    );
  }
  if (end < start) {
    throw new IntentReportValidationError(
      `${path}.lineRange`,
      `lineRange[1] (${end}) must be >= lineRange[0] (${start})`,
      i.lineRange,
    );
  }

  if (typeof i.intent !== "string" || i.intent.length === 0) {
    throw new IntentReportValidationError(
      `${path}.intent`,
      "expected non-empty string",
      i.intent,
    );
  }
  if (typeof i.hasRegressionTest !== "boolean") {
    throw new IntentReportValidationError(
      `${path}.hasRegressionTest`,
      "expected boolean",
      i.hasRegressionTest,
    );
  }
  if (typeof i.testGenerationOpportunity !== "boolean") {
    throw new IntentReportValidationError(
      `${path}.testGenerationOpportunity`,
      "expected boolean",
      i.testGenerationOpportunity,
    );
  }

  if (i.constraintCandidate !== null) {
    validateConstraintCandidate(i.constraintCandidate, `${path}.constraintCandidate`);
  }

  if (i.citations !== undefined) {
    if (!Array.isArray(i.citations)) {
      throw new IntentReportValidationError(
        `${path}.citations`,
        "expected array when present",
        i.citations,
      );
    }
    for (let k = 0; k < i.citations.length; k++) {
      validateCitation(i.citations[k], `${path}.citations[${k}]`);
    }
  }
}

function validateConstraintCandidate(value: unknown, path: string): void {
  if (!isPlainObject(value)) {
    throw new IntentReportValidationError(path, "expected object or null", value);
  }
  const c = value as Record<string, unknown>;

  if (typeof c.smtSketch !== "string" || c.smtSketch.length === 0) {
    throw new IntentReportValidationError(
      `${path}.smtSketch`,
      "expected non-empty string",
      c.smtSketch,
    );
  }
  if (typeof c.kind !== "string" || c.kind.length === 0) {
    throw new IntentReportValidationError(
      `${path}.kind`,
      "expected non-empty string",
      c.kind,
    );
  }
  const validStatuses = new Set([
    "candidate",
    "z3_sat",
    "passed_oracles",
    "rejected",
  ]);
  if (typeof c.validationStatus !== "string" || !validStatuses.has(c.validationStatus)) {
    throw new IntentReportValidationError(
      `${path}.validationStatus`,
      `expected one of ${[...validStatuses].join(" | ")}, got ${quote(c.validationStatus)}`,
      c.validationStatus,
    );
  }
}

function validateOutputBundle(value: unknown, path: string): void {
  if (!isPlainObject(value)) {
    throw new IntentReportValidationError(path, "expected object", value);
  }
  const b = value as Record<string, unknown>;

  if (b.patch !== null && typeof b.patch !== "string") {
    throw new IntentReportValidationError(
      `${path}.patch`,
      "expected string | null",
      b.patch,
    );
  }
  if (!Array.isArray(b.addedTests)) {
    throw new IntentReportValidationError(
      `${path}.addedTests`,
      "expected array",
      b.addedTests,
    );
  }
  for (let i = 0; i < b.addedTests.length; i++) {
    if (typeof b.addedTests[i] !== "string") {
      throw new IntentReportValidationError(
        `${path}.addedTests[${i}]`,
        "expected string",
        b.addedTests[i],
      );
    }
  }
  if (b.constraintArtifact !== null && typeof b.constraintArtifact !== "string") {
    throw new IntentReportValidationError(
      `${path}.constraintArtifact`,
      "expected string | null",
      b.constraintArtifact,
    );
  }
}

function validateCitation(value: unknown, path: string): void {
  if (!isPlainObject(value)) {
    throw new IntentReportValidationError(path, "expected object", value);
  }
  const c = value as Record<string, unknown>;
  if (typeof c.smtClause !== "string") {
    throw new IntentReportValidationError(
      `${path}.smtClause`,
      "expected string",
      c.smtClause,
    );
  }
  if (typeof c.sourceQuote !== "string") {
    throw new IntentReportValidationError(
      `${path}.sourceQuote`,
      "expected string",
      c.sourceQuote,
    );
  }
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function quote(v: unknown): string {
  if (typeof v === "string") return `"${v}"`;
  if (v === null) return "null";
  if (v === undefined) return "undefined";
  return String(v);
}
