/**
 * B1: "gap_report" intake adapter.
 *
 * Handles gap_reports rows produced by the SAST analyze pipeline.
 *
 * detect: returns 1.0 if input.context has a gap_report_id field.
 *
 * parse: mechanical — reads fields from input.context (no DB call in v1;
 *        the full gap row is expected in context). Optional LLM call for
 *        summary + failureDescription prose if not already present in context.
 *
 * Expected context shape:
 *   {
 *     gapReportId: string | number;
 *     reason: string;
 *     sourceLine?: string;       // "file.ts:42" or "file.ts:42:fn"
 *     principleId?: string;
 *     file?: string;
 *     line?: number;
 *     function?: string;
 *     db?: unknown;              // reserved for B2+ when DB reads are needed
 *   }
 */

import { registerIntakeAdapter } from "../intakeRegistry.js";
import type { IntakeInput, IntakeAdapter } from "../intakeRegistry.js";
import type { BugSignal, CodeReference, LLMProvider } from "../types.js";
import { requestStructuredJson } from "../llm/structuredOutput.js";

interface GapReportContext {
  gapReportId: string | number;
  reason: string;
  sourceLine?: string;
  principleId?: string;
  file?: string;
  line?: number;
  function?: string;
  db?: unknown;
}

function isGapReportContext(ctx: unknown): ctx is GapReportContext {
  return (
    typeof ctx === "object" &&
    ctx !== null &&
    "gapReportId" in ctx &&
    "reason" in ctx
  );
}

/** Parse "path/to/file.ts:42" or "path/to/file.ts:42:functionName" */
function parseSourceLine(sourceLine: string): CodeReference {
  const parts = sourceLine.split(":");
  const file = parts[0] ?? sourceLine;
  const line = parts[1] !== undefined ? parseInt(parts[1], 10) : undefined;
  const fn = parts[2] !== undefined && parts[2].trim() !== "" ? parts[2].trim() : undefined;
  return { file, line: isNaN(line ?? NaN) ? undefined : line, function: fn };
}

function buildSummaryPrompt(reason: string, file: string, line?: number): string {
  const loc = line !== undefined ? `${file}:${line}` : file;
  return (
    `You are a bug-report summarizer. Given a SAST gap finding, write a one-sentence summary\n` +
    `and a short failure description.\n\n` +
    `Finding: ${reason}\nLocation: ${loc}\n\n` +
    `Respond with a JSON object with keys "summary" (string) and "failureDescription" (string).\n` +
    `JSON only. No prose before or after.`
  );
}

const adapter: IntakeAdapter = {
  name: "gap_report",
  description: "SAST gap_reports rows produced by provekit analyze.",

  detect(input: IntakeInput): number {
    return isGapReportContext(input.context) ? 1.0 : 0;
  },

  async parse(input: IntakeInput, llm: LLMProvider): Promise<BugSignal> {
    if (!isGapReportContext(input.context)) {
      throw new Error(
        "gap_report adapter: input.context must have gapReportId and reason fields.",
      );
    }

    const ctx = input.context;

    // Build code reference mechanically.
    let codeRef: CodeReference | undefined;
    if (ctx.sourceLine) {
      codeRef = parseSourceLine(ctx.sourceLine);
    } else if (ctx.file) {
      codeRef = { file: ctx.file, line: ctx.line, function: ctx.function };
    }

    const codeReferences: CodeReference[] = codeRef ? [codeRef] : [];
    const file = codeRef?.file ?? "unknown";

    // LLM call for prose summary + failureDescription.
    const prompt = buildSummaryPrompt(ctx.reason, file, codeRef?.line);

    let prose: { summary: string; failureDescription: string };
    try {
      prose = await requestStructuredJson<{ summary: string; failureDescription: string }>({
        prompt,
        llm,
        stage: "intake-gapReport",
        model: "opus",
      });
    } catch {
      // Fallback: use raw reason text.
      prose = {
        summary: ctx.reason,
        failureDescription: ctx.reason,
      };
    }

    return {
      source: "gap_report",
      rawText: input.text || ctx.reason,
      summary: prose.summary,
      failureDescription: prose.failureDescription,
      codeReferences,
      bugClassHint: ctx.principleId,
    };
  },
};

export function registerGapReportIntakeAdapter(): void {
  registerIntakeAdapter(adapter);
}

// Self-register at module load.
registerGapReportIntakeAdapter();
