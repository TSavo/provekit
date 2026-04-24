/**
 * Fix-loop logger: structured, stage-aware, dual-output (stdout + file).
 *
 * - info/stage/llmCall/oracle/error always go to stdout and log file.
 * - detail goes to stdout only when verbose=true, always to log file.
 * - Log file gets full --verbose output regardless of stdout setting.
 * - close() must be called at the end of runFixLoopCli (use try/finally).
 */

import { mkdirSync, createWriteStream } from "fs";
import { dirname } from "path";
import type { WriteStream } from "fs";

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

  // fileStreamActive lets writeFile check if the stream is still usable after open errors
  let fileStreamActive = false;
  let fileStream: WriteStream | null = null;
  if (logFilePath) {
    try {
      mkdirSync(dirname(logFilePath), { recursive: true });
      fileStream = createWriteStream(logFilePath, { encoding: "utf-8", flags: "w" });
      fileStreamActive = true;
      // Swallow stream errors silently — log file is best-effort for debugging
      fileStream.on("error", () => { fileStreamActive = false; });
    } catch {
      // If we can't create the log dir/file, continue without file logging
      fileStream = null;
    }
  }

  const ts = () => new Date().toISOString();

  function writeStdout(line: string): void {
    stdout.write(line + "\n");
  }

  function writeFile(line: string): void {
    if (fileStream && fileStreamActive) {
      fileStream.write(line + "\n");
    }
  }

  function writeBoth(line: string): void {
    writeStdout(line);
    writeFile(line);
  }

  return {
    stage(name: string): void {
      const line = `\n━━━ ${name} ━━━`;
      writeBoth(line);
      writeFile(`[${ts()}]`);
    },

    info(msg: string): void {
      writeBoth(msg);
    },

    detail(msg: string): void {
      if (verbose) {
        writeStdout(`  ${msg}`);
      }
      writeFile(`  [detail] ${msg}`);
    },

    llmCall(a: LLMCallArgs): void {
      const summary = `  [llm:${a.stage}] model=${a.model} prompt=${a.promptLen}chars response=${a.responseLen}chars duration=${a.durationMs}ms`;
      writeBoth(summary);
    },

    oracle(a: OracleArgs): void {
      const icon = a.passed ? "PASS" : "FAIL";
      const line = `  [oracle #${a.id} ${a.name}] ${icon}: ${a.detail}`;
      writeBoth(line);
    },

    error(msg: string, context?: Record<string, unknown>): void {
      writeBoth(`  [ERROR] ${msg}`);
      if (context) {
        const json = JSON.stringify(context, null, 2);
        if (verbose) {
          writeStdout(json);
        }
        writeFile(json);
      }
    },

    close(): void {
      if (fileStream && fileStreamActive) {
        fileStreamActive = false;
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
  const response = await llm.complete(params);
  logger.llmCall({
    stage,
    model: params.model ?? "sonnet",
    promptLen: params.prompt.length,
    responseLen: response.length,
    durationMs: Date.now() - t0,
  });
  // Log full prompt/response to file via detail (always written to file)
  logger.detail(`[prompt]\n${"─".repeat(60)}\n${params.prompt}\n${"─".repeat(60)}`);
  logger.detail(`[response]\n${"─".repeat(60)}\n${response}\n${"─".repeat(60)}`);
  return response;
}
