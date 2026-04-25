/**
 * Fix-loop logger: structured, stage-aware, dual-output (stdout + file).
 *
 * Architecture:
 *   - Single pino root logger, level=trace.
 *   - File stream  (.provekit/fix-loop-<ts>.log): NDJSON, level=trace — EVERYTHING, no truncation.
 *   - Stdout stream (pino-pretty): level=info normally, level=debug when verbose=true.
 *
 * Rule: the file stream receives every event. The stdout stream is a filtered human-readable view.
 * Never truncate in the file stream. See docs/LOGGING.md.
 *
 * pino-roll is imported but rotation is not configured yet (placeholder for future use).
 */

import { mkdirSync } from "fs";
import { dirname } from "path";
import pino from "pino";
// pino-roll imported as placeholder — rotation not yet configured
// eslint-disable-next-line @typescript-eslint/no-require-imports
require("pino-roll"); // side-effect import to verify the dep is present

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

export interface LLMCallArgs {
  stage: string;
  model: string;
  promptLen: number;
  responseLen: number;
  durationMs: number;
}

export interface OracleArgs {
  id: number;
  name: string;
  passed: boolean;
  detail: string;
}

export interface FixLoopLogger {
  stage(name: string): void;
  info(msg: string): void;
  detail(msg: string): void;
  llmCall(args: LLMCallArgs): void;
  oracle(args: OracleArgs): void;
  error(msg: string, context?: Record<string, unknown>): void;
  close(): void;

  // Full LLM data — file only (trace level). Stdout never sees these payloads.
  prompt(stage: string, model: string, text: string): void;
  response(stage: string, model: string, text: string): void;
  toolUse(stage: string, tool: string, input: unknown): void;
  toolResult(stage: string, toolUseId: string, result: string): void;
  thinking(stage: string, content: string): void;
}

// ---------------------------------------------------------------------------
// Noop logger (for test paths and default backward compat)
// ---------------------------------------------------------------------------

export function createNoopLogger(): FixLoopLogger {
  return {
    stage: () => {},
    info: () => {},
    detail: () => {},
    llmCall: () => {},
    oracle: () => {},
    error: () => {},
    close: () => {},
    prompt: () => {},
    response: () => {},
    toolUse: () => {},
    toolResult: () => {},
    thinking: () => {},
  };
}

// ---------------------------------------------------------------------------
// Real logger factory
// ---------------------------------------------------------------------------

export function createFixLoopLogger(args: {
  stdout: NodeJS.WritableStream;
  verbose: boolean;
  logFilePath?: string;
}): FixLoopLogger {
  const { stdout, verbose, logFilePath } = args;

  // ── File destination ──────────────────────────────────────────────────────
  // We use sync: true so the fd is open immediately — this eliminates the
  // "sonic boom is not ready yet" race on close(), makes flushSync() safe to
  // call at any time, and guarantees that readFileSync() after close() sees all
  // entries. The fix loop is I/O-bound by LLM calls anyway so synchronous
  // log writes don't measurably affect throughput.
  //
  // pino-roll is the rotation mechanism (imported above as placeholder).
  let fileStream: ReturnType<typeof pino.destination> | null = null;

  if (logFilePath) {
    try {
      mkdirSync(dirname(logFilePath), { recursive: true });
      fileStream = pino.destination({ dest: logFilePath, sync: true });
    } catch {
      // If we can't create the log dir/file, continue without file logging.
      fileStream = null;
    }
  }

  // ── Stdout destination (pino-pretty) ──────────────────────────────────────
  // Use pino-pretty as a synchronous stream factory (NOT pino.transport worker
  // thread) so tests can inject a stub Writable via the stdout param.
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const PinoPretty = require("pino-pretty") as (opts: Record<string, unknown>) => NodeJS.WritableStream;

  const stdoutLevel: pino.LevelWithSilent = verbose ? "debug" : "info";
  // Colorize only when the destination is a real TTY.
  const colorize = Boolean((stdout as { isTTY?: boolean }).isTTY);

  const prettyStream = PinoPretty({
    destination: stdout,
    colorize,
    singleLine: true,
    // Suppress structural pino fields — our messageFormat rebuilds the human line.
    ignore: [
      "pid", "hostname", "time", "level",
      // Event-specific fields consumed by messageFormat:
      "event", "stage",
      "id", "name", "passed", "detail",
      "model", "promptLen", "responseLen", "durationMs",
      "context",
      "tool", "input",
      "toolUseId", "result",
      "content",
    ].join(","),
    messageFormat: (
      log: Record<string, unknown>,
      msgKey: string,
    ): string => {
      switch (log["event"]) {
        case "stage":
          return `\n━━━ ${String(log["stage"] ?? "")} ━━━`;
        case "llmCall":
          return (
            `  [llm:${String(log["stage"] ?? "")}] ` +
            `model=${String(log["model"] ?? "")} ` +
            `prompt=${String(log["promptLen"] ?? "")}chars ` +
            `response=${String(log["responseLen"] ?? "")}chars ` +
            `duration=${String(log["durationMs"] ?? "")}ms`
          );
        case "oracle": {
          const passed = log["passed"] ? "PASS" : "FAIL";
          return `  [oracle #${String(log["id"] ?? "")} ${String(log["name"] ?? "")}] ${passed}: ${String(log["detail"] ?? "")}`;
        }
        case "toolUse":
          return `  [tool:${String(log["tool"] ?? "")}] (input logged to file)`;
        case "toolResult":
          return `  [tool-result:${String(log["toolUseId"] ?? "")}] (result logged to file)`;
        default:
          return String(log[msgKey] ?? "");
      }
    },
  });

  // ── Root pino instance ────────────────────────────────────────────────────
  // stdoutLevel is info or debug (never trace or silent), so it fits pino.Level.
  const stdoutStreamLevel: pino.Level = stdoutLevel as pino.Level;
  const streams: pino.StreamEntry[] = [
    { level: stdoutStreamLevel, stream: prettyStream },
  ];
  if (fileStream) {
    streams.unshift({ level: "trace" as pino.Level, stream: fileStream });
  }

  // Set the root logger level to 'trace' so every child call reaches the
  // multistream router; individual stream entries control what gets emitted.
  const root = pino(
    { level: "trace" },
    pino.multistream(streams),
  );

  // Override prettyStream's effective level at the router level.
  // pino.multistream respects each entry's .level so we're good.

  // ── Impl ──────────────────────────────────────────────────────────────────

  // Build a child logger so each record can carry a "component" tag.
  const log = root.child({ component: "fix-loop" });

  return {
    stage(name: string): void {
      log.info({ event: "stage", stage: name }, "");
    },

    info(msg: string): void {
      log.info({ event: "info" }, msg);
    },

    detail(msg: string): void {
      // debug → stdout only when verbose. Always in file (trace ≤ debug ≤ info).
      log.debug({ event: "detail" }, msg);
    },

    llmCall(a: LLMCallArgs): void {
      log.info(
        {
          event: "llmCall",
          stage: a.stage,
          model: a.model,
          promptLen: a.promptLen,
          responseLen: a.responseLen,
          durationMs: a.durationMs,
        },
        "",
      );
    },

    oracle(a: OracleArgs): void {
      log.info(
        {
          event: "oracle",
          id: a.id,
          name: a.name,
          passed: a.passed,
          detail: a.detail,
        },
        "",
      );
    },

    error(msg: string, context?: Record<string, unknown>): void {
      log.error({ event: "error", context: context ?? {} }, msg);
    },

    // ── Full-payload methods — file only (trace), stdout never ──────────────

    prompt(stage: string, model: string, text: string): void {
      // Emitted at trace level: reaches file stream but NOT stdout (stdout min is info/debug).
      log.trace({ event: "prompt", stage, model, prompt: text }, "");
    },

    response(stage: string, model: string, text: string): void {
      log.trace({ event: "response", stage, model, response: text }, "");
    },

    toolUse(stage: string, tool: string, input: unknown): void {
      // trace → file only. debug → stdout summary line when verbose.
      log.trace({ event: "toolUsePayload", stage, tool, input }, "");
      log.debug({ event: "toolUse", stage, tool }, "");
    },

    toolResult(stage: string, toolUseId: string, result: string): void {
      log.trace({ event: "toolResultPayload", stage, toolUseId, result }, "");
      log.debug({ event: "toolResult", stage, toolUseId }, "");
    },

    thinking(stage: string, content: string): void {
      log.trace({ event: "thinking", stage, content }, "");
    },

    close(): void {
      if (fileStream) {
        // sync:true means the fd is open immediately, so flushSync() is always safe.
        fileStream.flushSync();
        fileStream.end();
        fileStream = null;
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Convenience: wrap an LLMProvider.complete call with timing + logging
// ---------------------------------------------------------------------------

import type { LLMProvider } from "./types.js";

export async function loggedComplete(
  logger: FixLoopLogger,
  stage: string,
  llm: LLMProvider,
  params: Parameters<LLMProvider["complete"]>[0],
): Promise<string> {
  const t0 = Date.now();

  // Log the full prompt to file (trace) before the call.
  logger.prompt(stage, params.model ?? "sonnet", params.prompt);

  const response = await llm.complete(params);

  logger.llmCall({
    stage,
    model: params.model ?? "sonnet",
    promptLen: params.prompt.length,
    responseLen: response.length,
    durationMs: Date.now() - t0,
  });

  // Log the full response to file (trace).
  logger.response(stage, params.model ?? "sonnet", response);

  return response;
}
