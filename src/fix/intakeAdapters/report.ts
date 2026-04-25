/**
 * B1: "report" intake adapter.
 *
 * Handles human-written bug reports: developer descriptions, GitHub issue
 * body text, or any unstructured prose describing a defect.
 *
 * detect: always returns 0.5 — this adapter is a fallback for any
 *         unstructured text that no other adapter claims with high confidence.
 *
 * parse: single LLM call with a structured-output prompt. Returns a JSON
 *        string that is parsed into BugSignal fields.
 */

import { registerIntakeAdapter } from "../intakeRegistry.js";
import type { IntakeInput, IntakeAdapter } from "../intakeRegistry.js";
import type { BugSignal, LLMProvider } from "../types.js";
import { parseJsonFromLlm } from "../llmJson.js";

function buildPrompt(text: string): string {
  return (
    `You are a bug-report parser. Given the following bug report, extract structured fields.\n` +
    `Respond with a JSON object containing exactly these keys:\n` +
    `  summary: string (one sentence)\n` +
    `  failureDescription: string (what goes wrong)\n` +
    `  fixHint: string | null (optional suggested fix)\n` +
    `  codeReferences: Array<{file: string, line?: number, function?: string}>\n` +
    `  bugClassHint: string | null (e.g. "null-dereference", "divide-by-zero")\n\n` +
    `Bug report:\n---\n${text}\n---\n` +
    `Respond with JSON only. No prose before or after.`
  );
}

const adapter: IntakeAdapter = {
  name: "report",
  description: "Human-written bug reports: developer descriptions, GitHub issue body text.",

  detect(_input: IntakeInput): number {
    // Fallback adapter — accepts any unstructured text at low confidence.
    return 0.5;
  },

  async parse(input: IntakeInput, llm: LLMProvider): Promise<BugSignal> {
    const prompt = buildPrompt(input.text);
    const raw = await llm.complete({ prompt, model: "opus" });

    let parsed: {
      summary: string;
      failureDescription: string;
      fixHint?: string | null;
      codeReferences?: Array<{ file: string; line?: number; function?: string }>;
      bugClassHint?: string | null;
    };
    try {
      parsed = parseJsonFromLlm(raw, "report");
    } catch (e) {
      throw new Error(e instanceof Error ? e.message : String(e));
    }

    return {
      source: "report",
      rawText: input.text,
      summary: parsed.summary ?? "",
      failureDescription: parsed.failureDescription ?? "",
      fixHint: parsed.fixHint ?? undefined,
      codeReferences: parsed.codeReferences ?? [],
      bugClassHint: parsed.bugClassHint ?? undefined,
    };
  },
};

export function registerReportIntakeAdapter(): void {
  registerIntakeAdapter(adapter);
}

// Self-register at module load.
registerReportIntakeAdapter();
