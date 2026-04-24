/**
 * B1: Intake adapter registry.
 *
 * The set of supported bug-report sources is a runtime registry — not a
 * hardcoded enum. Adding a new source is a registerIntakeAdapter() call with
 * no changes to parser or type code.
 *
 * Mirrors the shape of src/sast/capabilityRegistry.ts and
 * src/dsl/relationRegistry.ts.
 */

import type { BugSignal, LLMProvider } from "./types.js";

export interface IntakeInput {
  text: string;
  /**
   * Optional structured context. Adapter decides what shape it expects.
   * For "gap_report", this would be the gap_reports row + original code.
   * For "test_failure", this could be the Vitest failure object.
   */
  context?: unknown;
}

export interface IntakeAdapter {
  /** Source name as it appears in BugSignal.source. */
  name: string;
  /** Human-readable description of this source kind. */
  description: string;
  /**
   * Quick detect: given a raw input payload, can this adapter handle it?
   * Used for auto-routing when the caller doesn't specify the source.
   * Return a confidence score 0..1; false/null if definitely not this kind.
   */
  detect?: (input: IntakeInput) => number | boolean;
  /**
   * Parse the raw input into a BugSignal. May use the LLM for
   * unstructured-text sources; may be purely mechanical for structured ones.
   *
   * LLM adapters return BugSignal fields encoded as JSON strings from the
   * LLM provider — adapters are responsible for JSON.parse().
   */
  parse: (input: IntakeInput, llm: LLMProvider) => Promise<BugSignal>;
}

const registry = new Map<string, IntakeAdapter>();

/**
 * Register an intake adapter. Idempotent: duplicate names overwrite (with a warning).
 */
export function registerIntakeAdapter(a: IntakeAdapter): void {
  if (registry.has(a.name)) {
    console.warn(`[intakeRegistry] duplicate registration for "${a.name}"; overwriting.`);
  }
  registry.set(a.name, a);
}

/** Look up an adapter by source name. Returns undefined if not registered. */
export function getIntakeAdapter(name: string): IntakeAdapter | undefined {
  return registry.get(name);
}

/** All registered adapters (read-only snapshot). */
export function listIntakeAdapters(): readonly IntakeAdapter[] {
  return Array.from(registry.values());
}

/** Clear the registry. ONLY for tests. */
export function _clearIntakeRegistry(): void {
  registry.clear();
}
