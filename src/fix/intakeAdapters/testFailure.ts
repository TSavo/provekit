/**
 * B1: "test_failure" intake adapter.
 *
 * Handles Vitest/Jest failure objects passed as structured context.
 *
 * detect: returns 1.0 if input.context has testName + errorMessage fields.
 *
 * parse: extract test name, error message, stack trace. File+line from stack.
 *        LLM call for summary/failureDescription prose.
 *
 * Expected context shape:
 *   {
 *     testName: string;
 *     errorMessage: string;
 *     stack?: string;    // raw stack trace string
 *   }
 */

import { registerIntakeAdapter } from "../intakeRegistry.js";
import type { IntakeInput, IntakeAdapter } from "../intakeRegistry.js";
import type { BugSignal, CodeReference, LLMProvider } from "../types.js";
import { parseJsonFromLlm } from "../llmJson.js";

interface TestFailureContext {
  testName: string;
  errorMessage: string;
  stack?: string;
}

function isTestFailureContext(ctx: unknown): ctx is TestFailureContext {
  return (
    typeof ctx === "object" &&
    ctx !== null &&
    "testName" in ctx &&
    "errorMessage" in ctx
  );
}

/**
 * Parse stack trace lines like:
 *   at Object.<anonymous> (/path/to/file.ts:42:7)
 *   at functionName (file.ts:10:3)
 *   at file.ts:5:1
 */
function parseStackFrames(stack: string): CodeReference[] {
  const refs: CodeReference[] = [];
  // Match: "at <optional-name> (<file>:<line>:<col>)" or "at <file>:<line>:<col>"
  const frameRe = /at\s+(?:([^\s(]+)\s+\()?([^():]+\.[a-z]+):(\d+)(?::\d+)?\)?/gi;
  let match: RegExpExecArray | null;
  while ((match = frameRe.exec(stack)) !== null) {
    const fn = match[1] && match[1] !== "Object.<anonymous>" ? match[1] : undefined;
    const file = match[2];
    const line = parseInt(match[3], 10);
    if (file && !isNaN(line)) {
      refs.push({ file, line, function: fn });
    }
  }
  return refs;
}

function buildPrompt(testName: string, errorMessage: string, stack?: string): string {
  const stackSection = stack ? `\nStack:\n${stack.slice(0, 800)}` : "";
  return (
    `You are a test-failure summarizer. Given a failing test, write a one-sentence summary\n` +
    `and a short failure description.\n\n` +
    `Test: ${testName}\nError: ${errorMessage}${stackSection}\n\n` +
    `Respond with a JSON object with keys "summary" (string) and "failureDescription" (string).\n` +
    `JSON only. No prose before or after.`
  );
}

const adapter: IntakeAdapter = {
  name: "test_failure",
  description: "Vitest/Jest test failure objects with testName, errorMessage, and optional stack.",

  detect(input: IntakeInput): number {
    return isTestFailureContext(input.context) ? 1.0 : 0;
  },

  async parse(input: IntakeInput, llm: LLMProvider): Promise<BugSignal> {
    if (!isTestFailureContext(input.context)) {
      throw new Error(
        "test_failure adapter: input.context must have testName and errorMessage fields.",
      );
    }

    const ctx = input.context;
    const codeReferences = ctx.stack ? parseStackFrames(ctx.stack) : [];

    const prompt = buildPrompt(ctx.testName, ctx.errorMessage, ctx.stack);
    const raw = await llm.complete({ prompt, model: "opus" });

    let prose: { summary: string; failureDescription: string };
    try {
      prose = parseJsonFromLlm(raw, "testFailure");
    } catch {
      prose = {
        summary: `Test "${ctx.testName}" failed: ${ctx.errorMessage}`,
        failureDescription: ctx.errorMessage,
      };
    }

    return {
      source: "test_failure",
      rawText: input.text || `${ctx.testName}: ${ctx.errorMessage}`,
      summary: prose.summary,
      failureDescription: prose.failureDescription,
      codeReferences,
      bugClassHint: "test-failure",
    };
  },
};

export function registerTestFailureIntakeAdapter(): void {
  registerIntakeAdapter(adapter);
}

// Self-register at module load.
registerTestFailureIntakeAdapter();
