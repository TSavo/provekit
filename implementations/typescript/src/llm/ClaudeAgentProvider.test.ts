/**
 * ClaudeAgentProvider.test.ts — unit tests for tool_use, thinking, and text block capture.
 *
 * Mocks @anthropic-ai/claude-agent-sdk to yield a synthetic message stream
 * with mixed text + tool_use + thinking blocks. Verifies toolUses population,
 * file paths, full (un-truncated) result content, thinking block capture, and
 * console.log summaries.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ---------------------------------------------------------------------------
// Mock the SDK before importing the provider.
// ---------------------------------------------------------------------------

vi.mock("@anthropic-ai/claude-agent-sdk", () => ({
  query: vi.fn(),
}));

// Mock child_process so git calls don't fail in the test environment.
vi.mock("child_process", () => ({
  execFileSync: vi.fn(() => ""),
}));

import { ClaudeAgentProvider } from "./ClaudeAgentProvider";
import { query } from "@anthropic-ai/claude-agent-sdk";

const mockQuery = query as ReturnType<typeof vi.fn>;

// ---------------------------------------------------------------------------
// Helpers to build synthetic SDK message streams.
// ---------------------------------------------------------------------------

async function* makeStream(messages: unknown[]): AsyncIterable<unknown> {
  for (const m of messages) yield m;
}

function assistantWithBlocks(blocks: unknown[]): unknown {
  return {
    type: "assistant",
    message: { content: blocks },
  };
}

function userWithToolResults(results: Array<{ tool_use_id: string; content: unknown; is_error?: boolean }>): unknown {
  return {
    type: "user",
    message: {
      content: results.map((r) => ({
        type: "tool_result",
        tool_use_id: r.tool_use_id,
        content: r.content,
        is_error: r.is_error ?? false,
      })),
    },
  };
}

function successResult(text: string): unknown {
  return { type: "result", subtype: "success", result: text };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("ClaudeAgentProvider.agent — tool_use capture", () => {
  let provider: ClaudeAgentProvider;
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    provider = new ClaudeAgentProvider();
    consoleSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    vi.clearAllMocks();
  });

  it("populates toolUses from a mixed text+tool_use stream", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "text", text: "I will edit the file." },
        { type: "tool_use", id: "tu_1", name: "Edit", input: { file_path: "/abs/src/foo.ts", old_string: "hello", new_string: "world" } },
      ]),
      userWithToolResults([{ tool_use_id: "tu_1", content: "ok" }]),
      assistantWithBlocks([
        { type: "tool_use", id: "tu_2", name: "Bash", input: { command: "git status --porcelain" } },
      ]),
      userWithToolResults([{ tool_use_id: "tu_2", content: "M src/foo.ts\n" }]),
      successResult("done"),
    ]));

    const result = await provider.agent("fix it", { cwd: "/tmp/repo" });

    expect(result.toolUses).toHaveLength(2);

    const edit = result.toolUses[0];
    expect(edit.name).toBe("Edit");
    expect((edit.input as any).file_path).toBe("/abs/src/foo.ts");
    expect(edit.isError).toBe(false);
    expect(edit.turn).toBe(1);
    expect(edit.result).toBe("ok");
    expect(typeof edit.ms).toBe("number");

    const bash = result.toolUses[1];
    expect(bash.name).toBe("Bash");
    expect((bash.input as any).command).toBe("git status --porcelain");
    expect(bash.turn).toBe(2);
  });

  it("captures absolute file paths on Edit and Write", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "tool_use", id: "tw_1", name: "Write", input: { file_path: "/outside/sandbox/evil.ts", content: "x" } },
      ]),
      userWithToolResults([{ tool_use_id: "tw_1", content: "written" }]),
      successResult("done"),
    ]));

    const result = await provider.agent("write file", { cwd: "/tmp/repo" });

    expect(result.toolUses).toHaveLength(1);
    expect((result.toolUses[0].input as any).file_path).toBe("/outside/sandbox/evil.ts");
  });

  it("stores full result content without truncation", async () => {
    const longContent = "x".repeat(1000);
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "tool_use", id: "tr_1", name: "Read", input: { file_path: "/src/big.ts" } },
      ]),
      userWithToolResults([{ tool_use_id: "tr_1", content: longContent }]),
      successResult("done"),
    ]));

    const result = await provider.agent("read file", { cwd: "/tmp/repo" });

    // Full content — no truncation. Per docs/LOGGING.md.
    expect(result.toolUses[0].result).toBe(longContent);
    expect(result.toolUses[0].result!.length).toBe(1000);
  });

  it("handles array-form tool_result content without truncation", async () => {
    const longText = "y".repeat(800);
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "tool_use", id: "ta_1", name: "Read", input: { file_path: "/src/arr.ts" } },
      ]),
      userWithToolResults([{
        tool_use_id: "ta_1",
        content: [{ type: "text", text: longText }],
      }]),
      successResult("done"),
    ]));

    const result = await provider.agent("read array", { cwd: "/tmp/repo" });

    // Full content — no truncation. Per docs/LOGGING.md.
    expect(result.toolUses[0].result).toBe(longText);
    expect(result.toolUses[0].result!.length).toBe(800);
  });

  it("sets isError=true for error tool results", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "tool_use", id: "te_1", name: "Bash", input: { command: "cat /etc/shadow" } },
      ]),
      userWithToolResults([{ tool_use_id: "te_1", content: "permission denied", is_error: true }]),
      successResult("done"),
    ]));

    const result = await provider.agent("bad command", { cwd: "/tmp/repo" });

    expect(result.toolUses[0].isError).toBe(true);
  });

  it("emits concise console.log for each tool_use", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "tool_use", id: "tl_1", name: "Edit", input: { file_path: "/src/bar.ts", old_string: "ab", new_string: "cde" } },
        { type: "tool_use", id: "tl_2", name: "Bash", input: { command: "npm test" } },
      ]),
      userWithToolResults([
        { tool_use_id: "tl_1", content: "ok" },
        { tool_use_id: "tl_2", content: "PASS" },
      ]),
      successResult("done"),
    ]));

    await provider.agent("run edits", { cwd: "/tmp/repo" });

    const logs = consoleSpy.mock.calls.map((c: unknown[]) => c[0] as string);
    expect(logs.some((l: string) => l.includes("tool_use #1: Edit(/src/bar.ts, -2/3 chars)"))).toBe(true);
    expect(logs.some((l: string) => l.includes("tool_use #2: Bash($ npm test)"))).toBe(true);
  });

  it("returns empty toolUses when stream has no tool_use blocks", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([{ type: "text", text: "no tools needed" }]),
      successResult("done"),
    ]));

    const result = await provider.agent("no tools", { cwd: "/tmp/repo" });

    expect(result.toolUses).toEqual([]);
  });

  it("flushes unmatched tool_use (no tool_result) into toolUses", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "tool_use", id: "tu_orphan", name: "Glob", input: { pattern: "**/*.ts", path: "src" } },
      ]),
      // No user message with tool_result
      successResult("done"),
    ]));

    const result = await provider.agent("orphan tool", { cwd: "/tmp/repo" });

    expect(result.toolUses).toHaveLength(1);
    expect(result.toolUses[0].name).toBe("Glob");
    expect(result.toolUses[0].result).toBeUndefined();
  });

  it("captures thinking blocks per turn", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "thinking", thinking: "I should edit foo.ts to fix the null check." },
        { type: "text", text: "Let me fix the null check." },
        { type: "tool_use", id: "tt_1", name: "Edit", input: { file_path: "/src/foo.ts", old_string: "a", new_string: "b" } },
      ]),
      userWithToolResults([{ tool_use_id: "tt_1", content: "ok" }]),
      assistantWithBlocks([
        { type: "thinking", thinking: "The edit looks good. I am done." },
        { type: "text", text: "Done." },
      ]),
      successResult("fixed"),
    ]));

    const result = await provider.agent("fix null check", { cwd: "/tmp/repo" });

    expect(result.thinkingBlocks).toHaveLength(2);
    expect(result.thinkingBlocks[0]).toEqual({ turn: 1, content: "I should edit foo.ts to fix the null check." });
    expect(result.thinkingBlocks[1]).toEqual({ turn: 2, content: "The edit looks good. I am done." });

    expect(result.textBlocks).toHaveLength(2);
    expect(result.textBlocks[0]).toEqual({ turn: 1, content: "Let me fix the null check." });
    expect(result.textBlocks[1]).toEqual({ turn: 2, content: "Done." });
  });

  it("captures redacted_thinking blocks as [redacted]", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([
        { type: "redacted_thinking", data: "encrypted-blob" },
        { type: "text", text: "Here is my answer." },
      ]),
      successResult("answer"),
    ]));

    const result = await provider.agent("sensitive question", { cwd: "/tmp/repo" });

    expect(result.thinkingBlocks).toHaveLength(1);
    expect(result.thinkingBlocks[0]).toEqual({ turn: 1, content: "[redacted]" });
  });

  it("returns empty thinkingBlocks and textBlocks when stream has no such blocks", async () => {
    mockQuery.mockReturnValue(makeStream([
      assistantWithBlocks([{ type: "text", text: "simple response" }]),
      successResult("done"),
    ]));

    const result = await provider.agent("simple", { cwd: "/tmp/repo" });

    expect(result.thinkingBlocks).toEqual([]);
    expect(result.textBlocks).toHaveLength(1);
    expect(result.textBlocks[0].content).toBe("simple response");
  });
});
