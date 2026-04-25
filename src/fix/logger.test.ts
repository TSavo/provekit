/**
 * Tests for the pino-based FixLoopLogger.
 *
 * Key invariants checked:
 *   1. File stream (trace level) records debug entries that stdout (info level) drops.
 *   2. Stage markers appear on stdout.
 *   3. prompt/response/thinking go to file only — never stdout (default verbose=false).
 *   4. With verbose=true, toolUse summary lines appear on stdout.
 *   5. close() flushes the file so reads after close() see all entries.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdirSync, readFileSync, rmSync, existsSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { Writable } from "stream";
import { createFixLoopLogger, createNoopLogger } from "./logger.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function captureStream(): { stream: NodeJS.WritableStream; captured(): string } {
  const chunks: string[] = [];
  const stream = new Writable({
    write(chunk: Buffer | string, _enc: BufferEncoding, cb: () => void) {
      chunks.push(typeof chunk === "string" ? chunk : chunk.toString("utf-8"));
      cb();
    },
  }) as NodeJS.WritableStream;
  return {
    stream,
    captured: () => chunks.join(""),
  };
}

function tempLogPath(label: string): string {
  const dir = join(tmpdir(), "provekit-logger-test");
  mkdirSync(dir, { recursive: true });
  return join(dir, `${label}-${Date.now()}.log`);
}

afterEach(() => {
  const dir = join(tmpdir(), "provekit-logger-test");
  if (existsSync(dir)) {
    rmSync(dir, { recursive: true, force: true });
  }
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("createFixLoopLogger", () => {

  it("file contains debug-level entry that stdout does NOT print (trace > stdout-info)", async () => {
    const logFile = tempLogPath("debug-only");
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false, logFilePath: logFile });
    logger.detail("this is a debug-only message");
    logger.close();

    const fileContent = readFileSync(logFile, "utf-8");
    const stdoutContent = captured();

    expect(fileContent).toContain("debug-only message");
    expect(stdoutContent).not.toContain("debug-only message");
  });

  it("stage marker appears on stdout", () => {
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false });
    logger.stage("Intake");
    logger.close();

    expect(captured()).toContain("━━━ Intake ━━━");
  });

  it("stage marker is recorded in file as structured event", () => {
    const logFile = tempLogPath("stage");
    const { stream: out } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false, logFilePath: logFile });
    logger.stage("Locate");
    logger.close();

    const fileContent = readFileSync(logFile, "utf-8");
    const lines = fileContent.trim().split("\n").filter(Boolean);
    const stageLine = lines.find((l) => {
      try { return JSON.parse(l).event === "stage"; } catch { return false; }
    });
    expect(stageLine).toBeDefined();
    const record = JSON.parse(stageLine!);
    expect(record.stage).toBe("Locate");
  });

  it("prompt/response/thinking write to file but NOT stdout (verbose=false)", () => {
    const logFile = tempLogPath("prompt-only");
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false, logFilePath: logFile });
    logger.prompt("C3", "sonnet", "full prompt text here");
    logger.response("C3", "sonnet", "full response text here");
    logger.thinking("C3", "full thinking block content here");
    logger.close();

    const fileContent = readFileSync(logFile, "utf-8");
    const stdoutContent = captured();

    // File: must contain all full text
    expect(fileContent).toContain("full prompt text here");
    expect(fileContent).toContain("full response text here");
    expect(fileContent).toContain("full thinking block content here");

    // Stdout: must not contain any of these (trace-only)
    expect(stdoutContent).not.toContain("full prompt text here");
    expect(stdoutContent).not.toContain("full response text here");
    expect(stdoutContent).not.toContain("full thinking block content here");
  });

  it("toolUse summaries appear on stdout with verbose=true", () => {
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: true });
    logger.toolUse("C3", "Bash", { command: "ls -la" });
    logger.close();

    expect(captured()).toContain("tool:Bash");
  });

  it("toolUse payload (full input) is in file even when verbose=false", () => {
    const logFile = tempLogPath("tooluse-payload");
    const { stream: out } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false, logFilePath: logFile });
    logger.toolUse("C3", "Edit", { old_string: "the entire old string", new_string: "the entire new string" });
    logger.close();

    const fileContent = readFileSync(logFile, "utf-8");
    expect(fileContent).toContain("the entire old string");
    expect(fileContent).toContain("the entire new string");
  });

  it("thinking goes to file only — never stdout (verbose=false or true)", () => {
    const logFile = tempLogPath("thinking-only");
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: true, logFilePath: logFile });
    logger.thinking("C3", "long reasoning text about the fix approach");
    logger.close();

    const fileContent = readFileSync(logFile, "utf-8");
    const stdoutContent = captured();

    // File: must contain full thinking content
    expect(fileContent).toContain("long reasoning text about the fix approach");
    // Stdout: never (trace level)
    expect(stdoutContent).not.toContain("long reasoning text about the fix approach");
  });

  it("toolUse: file gets full input (stringified), stdout gets one-line summary at debug level", () => {
    const logFile = tempLogPath("tooluse-full-vs-summary");
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: true, logFilePath: logFile });
    logger.toolUse("C3", "Edit", { file_path: "/x", old_string: "entire old string content", new_string: "entire new string content" });
    logger.close();

    const fileContent = readFileSync(logFile, "utf-8");
    const stdoutContent = captured();

    // File: must contain entire input (no truncation)
    expect(fileContent).toContain("entire old string content");
    expect(fileContent).toContain("entire new string content");
    // Stdout: summary line (event=toolUse) at debug level (visible in verbose=true)
    expect(stdoutContent).toContain("tool:Edit");
    // Stdout: must NOT contain the raw string contents (those go to file only via trace)
    expect(stdoutContent).not.toContain("entire old string content");
  });

  it("toolResult full payload is in file, never stdout", () => {
    const logFile = tempLogPath("toolresult");
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false, logFilePath: logFile });
    logger.toolResult("C3", "tool-use-abc123", "full result body content here");
    logger.close();

    const fileContent = readFileSync(logFile, "utf-8");
    const stdoutContent = captured();

    expect(fileContent).toContain("full result body content here");
    expect(stdoutContent).not.toContain("full result body content here");
  });

  it("close() flushes the file (reads after close see all entries)", () => {
    const logFile = tempLogPath("flush");
    const { stream: out } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false, logFilePath: logFile });
    logger.info("before close message");
    logger.close();

    // File must be readable and contain the entry immediately after close()
    const fileContent = readFileSync(logFile, "utf-8");
    expect(fileContent).toContain("before close message");
  });

  it("llmCall appears on stdout as formatted summary", () => {
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false });
    logger.llmCall({ stage: "Classify", model: "claude-sonnet-4-5", promptLen: 2048, responseLen: 512, durationMs: 1200 });
    logger.close();

    const stdoutContent = captured();
    expect(stdoutContent).toContain("[llm:Classify]");
    expect(stdoutContent).toContain("model=claude-sonnet-4-5");
  });

  it("oracle appears on stdout with PASS/FAIL indicator", () => {
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false });
    logger.oracle({ id: 3, name: "null-check", passed: false, detail: "returned null" });
    logger.close();

    expect(captured()).toContain("[oracle #3 null-check] FAIL: returned null");
  });

  it("error appears on stdout", () => {
    const { stream: out, captured } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false });
    logger.error("something exploded", { code: 42 });
    logger.close();

    expect(captured()).toContain("something exploded");
  });

  it("file records are valid NDJSON (no split records)", () => {
    const logFile = tempLogPath("ndjson");
    const { stream: out } = captureStream();

    const logger = createFixLoopLogger({ stdout: out, verbose: false, logFilePath: logFile });
    logger.stage("A");
    logger.info("test message");
    logger.detail("detail message");
    logger.llmCall({ stage: "A", model: "m", promptLen: 100, responseLen: 50, durationMs: 10 });
    logger.close();

    const lines = readFileSync(logFile, "utf-8").trim().split("\n").filter(Boolean);
    for (const line of lines) {
      expect(() => JSON.parse(line)).not.toThrow();
    }
    expect(lines.length).toBeGreaterThanOrEqual(3);
  });
});

describe("createNoopLogger", () => {
  it("all methods are callable without throwing", () => {
    const noop = createNoopLogger();
    expect(() => {
      noop.stage("X");
      noop.info("msg");
      noop.detail("d");
      noop.llmCall({ stage: "X", model: "m", promptLen: 0, responseLen: 0, durationMs: 0 });
      noop.oracle({ id: 1, name: "n", passed: true, detail: "ok" });
      noop.error("err");
      noop.prompt("X", "m", "text");
      noop.response("X", "m", "text");
      noop.toolUse("X", "Bash", {});
      noop.toolResult("X", "id", "result");
      noop.thinking("X", "thoughts");
      noop.close();
    }).not.toThrow();
  });
});
