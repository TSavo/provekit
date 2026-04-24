/**
 * B1: "runtime_log" intake adapter.
 *
 * Handles production log lines or runtime error traces (e.g., a Sentry event,
 * a Node.js uncaught exception dump, or a plain stack trace paste).
 *
 * detect: returns 0.7 if text looks like a stack trace (regex on "at X:Y:Z" patterns).
 *
 * parse: extract stack frames → codeReferences via regex. LLM call for summary
 *        + bug classification hint.
 */

import { registerIntakeAdapter } from "../intakeRegistry.js";
import type { IntakeInput, IntakeAdapter } from "../intakeRegistry.js";
import type { BugSignal, CodeReference, LLMProvider } from "../types.js";

/**
 * Detect stack-trace-style lines:
 *   at SomeFunction (/path/file.ts:10:3)
 *   at /path/file.ts:10:3
 *   at Object.<anonymous> (file.js:5:1)
 */
const STACK_FRAME_RE = /at\s+(?:[^\s(]+\s+\()?[^\s():]+\.[a-z]+:\d+(?::\d+)?\)?/i;

/**
 * Full extractor — same logic as testFailure, kept here to avoid coupling.
 * Matches: at <optional-name> (<file>:<line>:<col>)  or  at <file>:<line>
 */
function extractStackFrames(text: string): CodeReference[] {
  const refs: CodeReference[] = [];
  const frameRe = /at\s+(?:([^\s(]+)\s+\()?([^():]+\.[a-z]+):(\d+)(?::\d+)?\)?/gi;
  let match: RegExpExecArray | null;
  while ((match = frameRe.exec(text)) !== null) {
    const fn = match[1] && match[1] !== "Object.<anonymous>" ? match[1] : undefined;
    const file = match[2];
    const line = parseInt(match[3], 10);
    if (file && !isNaN(line)) {
      refs.push({ file, line, function: fn });
    }
  }
  return refs;
}

function buildPrompt(text: string): string {
  return (
    `You are a runtime-log analyst. Given a production log or stack trace, extract:\n` +
    `  summary: one-sentence description of the error\n` +
    `  failureDescription: what went wrong in detail\n` +
    `  bugClassHint: a bug category label (e.g. "null-dereference", "unhandled-promise", "memory-leak")\n\n` +
    `Log:\n---\n${text.slice(0, 1500)}\n---\n` +
    `Respond with a JSON object with keys "summary", "failureDescription", "bugClassHint".\n` +
    `JSON only. No prose before or after.`
  );
}

const adapter: IntakeAdapter = {
  name: "runtime_log",
  description: "Production log lines or runtime error traces (stack traces, Sentry events).",

  detect(input: IntakeInput): number {
    // 0.7 if the text looks like a stack trace, 0 otherwise.
    return STACK_FRAME_RE.test(input.text) ? 0.7 : 0;
  },

  async parse(input: IntakeInput, llm: LLMProvider): Promise<BugSignal> {
    const codeReferences = extractStackFrames(input.text);

    const prompt = buildPrompt(input.text);
    const raw = await llm.complete({ prompt, model: "haiku" });

    let parsed: { summary: string; failureDescription: string; bugClassHint?: string | null };
    try {
      parsed = JSON.parse(raw) as typeof parsed;
    } catch {
      parsed = {
        summary: input.text.split("\n")[0] ?? input.text.slice(0, 120),
        failureDescription: input.text.slice(0, 400),
        bugClassHint: undefined,
      };
    }

    return {
      source: "runtime_log",
      rawText: input.text,
      summary: parsed.summary,
      failureDescription: parsed.failureDescription,
      codeReferences,
      bugClassHint: parsed.bugClassHint ?? undefined,
    };
  },
};

export function registerRuntimeLogIntakeAdapter(): void {
  registerIntakeAdapter(adapter);
}

// Self-register at module load.
registerRuntimeLogIntakeAdapter();
