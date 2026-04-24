/**
 * B1: Main entry point for bug-signal intake.
 *
 * Thin router — all parsing logic lives in adapters. This file imports all
 * four v1 adapters (triggering their self-registration) and routes through
 * the intake adapter registry.
 *
 * LLM convention: adapters receive a string from LLMProvider.complete() and
 * must JSON.parse() it themselves. The stub contract is: pass a JSON string
 * as the canned response. Adapters handle malformed JSON gracefully.
 */

import "./intakeAdapters/index.js";

import {
  getIntakeAdapter,
  listIntakeAdapters,
} from "./intakeRegistry.js";
import type { BugSignal, LLMProvider } from "./types.js";
import type { IntakeInput } from "./intakeRegistry.js";

export type { BugSignal, LLMProvider } from "./types.js";
export { StubLLMProvider } from "./types.js";
export {
  registerIntakeAdapter,
  getIntakeAdapter,
  listIntakeAdapters,
  _clearIntakeRegistry,
} from "./intakeRegistry.js";
export type { IntakeAdapter, IntakeInput } from "./intakeRegistry.js";

/**
 * Parse a bug signal using the named adapter.
 * Throws with a list of registered source names if the source is unknown.
 */
export async function parseBugSignal(
  input: { text: string; source: string; context?: unknown },
  llm: LLMProvider,
): Promise<BugSignal> {
  const adapter = getIntakeAdapter(input.source);
  if (!adapter) {
    const registered = listIntakeAdapters()
      .map((a) => a.name)
      .join(", ");
    throw new Error(
      `unknown intake source '${input.source}'. Registered: ${registered}`,
    );
  }
  const intakeInput: IntakeInput = { text: input.text, context: input.context };
  return adapter.parse(intakeInput, llm);
}

/**
 * Auto-detect the source adapter by highest detect() score, then parse.
 * Falls back to the "report" adapter (score 0.5) if no adapter scores higher.
 * Throws if no adapters are registered or all score 0.
 */
export async function detectAndParseBugSignal(
  input: { text: string; context?: unknown },
  llm: LLMProvider,
): Promise<BugSignal> {
  const adapters = listIntakeAdapters();
  if (adapters.length === 0) {
    throw new Error("detectAndParseBugSignal: no intake adapters registered.");
  }

  const intakeInput: IntakeInput = { text: input.text, context: input.context };

  let bestAdapter = adapters[0];
  let bestScore: number = -1;

  for (const adapter of adapters) {
    if (!adapter.detect) continue;
    const raw = adapter.detect(intakeInput);
    const score = typeof raw === "boolean" ? (raw ? 1 : 0) : raw;
    if (score > bestScore) {
      bestScore = score;
      bestAdapter = adapter;
    }
  }

  // If no adapter had a detect function, use the first one.
  if (bestScore < 0) {
    bestAdapter = adapters[0];
  }

  return bestAdapter.parse(intakeInput, llm);
}
