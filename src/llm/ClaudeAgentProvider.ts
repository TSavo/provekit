import { query } from "@anthropic-ai/claude-agent-sdk";
import { execFileSync } from "child_process";
import { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent, AgentRequestOptions, AgentResult, ToolUseRecord } from "./Provider";

export class ClaudeAgentProvider implements LLMProvider {
  readonly name = "claude-agent-sdk";

  async complete(prompt: string, options: LLMRequestOptions): Promise<LLMResponse> {
    console.log(`[llm:${this.name}] Sending ${prompt.length} chars to ${options.model}`);
    const startTime = Date.now();

    let text = "";
    for await (const message of query({
      prompt,
      options: {
        model: options.model,
        systemPrompt: options.systemPrompt,
      },
    })) {
      if (message.type === "assistant") {
        const content = (message as any).message?.content;
        if (Array.isArray(content)) {
          text += content
            .filter((b: any) => b.type === "text")
            .map((b: any) => b.text)
            .join("");
        }
      }
      if (message.type === "result" && message.subtype === "success") {
        text = message.result;
      }
    }

    console.log(`[llm:${this.name}] Response received in ${Date.now() - startTime}ms (${text.length} chars)`);
    return { text };
  }

  async *stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent> {
    for await (const message of query({
      prompt,
      options: {
        model: options.model,
        includePartialMessages: true,
        systemPrompt: options.systemPrompt,
      },
    })) {
      if (message.type === "stream_event") {
        const event = (message as any).event;
        if (event?.type === "content_block_delta" && event.delta?.type === "text_delta") {
          yield { type: "text_delta", text: event.delta.text };
        }
      }
      if (message.type === "result" && message.subtype === "success") {
        yield { type: "done", text: message.result };
      }
    }
  }

  async agent(prompt: string, options: AgentRequestOptions): Promise<AgentResult> {
    const { cwd } = options;
    const allowedTools = options.allowedTools ?? ["Read", "Edit", "Write", "Bash", "Glob", "Grep"];
    const maxTurns = options.maxTurns ?? 20;

    // Map tier alias to a model string. The SDK accepts full model IDs or aliases.
    const modelMap: Record<string, string> = {
      haiku: "claude-haiku-4-5",
      sonnet: "claude-sonnet-4-5",
      opus: "claude-opus-4-7",
    };
    const model = options.model ? (modelMap[options.model] ?? options.model) : undefined;

    let turnsUsed = 0;
    let finalText = "";
    const toolUses: ToolUseRecord[] = [];
    const thinkingBlocks: Array<{ turn: number; content: string }> = [];
    const textBlocks: Array<{ turn: number; content: string }> = [];

    // Pending tool calls: keyed by tool_use id, value is { turn, startMs }.
    const pendingTools = new Map<string, { turn: number; startMs: number; name: string; input: unknown }>();

    let toolUseCounter = 0;

    for await (const message of query({
      prompt,
      options: {
        model,
        cwd,
        allowedTools,
        maxTurns,
        systemPrompt: options.systemPrompt,
        permissionMode: "acceptEdits",
        thinking: { type: "enabled", budgetTokens: 4096 },
      },
    })) {
      if (message.type === "assistant") {
        turnsUsed++;
        const content = (message as any).message?.content;
        if (Array.isArray(content)) {
          for (const block of content) {
            if (block.type === "thinking") {
              thinkingBlocks.push({ turn: turnsUsed, content: (block as any).thinking ?? "" });
            } else if (block.type === "redacted_thinking") {
              thinkingBlocks.push({ turn: turnsUsed, content: "[redacted]" });
            } else if (block.type === "text") {
              textBlocks.push({ turn: turnsUsed, content: (block as any).text ?? "" });
            } else if (block.type === "tool_use") {
              toolUseCounter++;
              pendingTools.set(block.id, {
                turn: turnsUsed,
                startMs: Date.now(),
                name: block.name,
                input: block.input,
              });
              const summary = formatToolSummary(block.name, block.input);
              console.log(`[llm:${this.name}] tool_use #${toolUseCounter}: ${block.name}(${summary})`);
            }
          }
        }
      }

      if (message.type === "user") {
        const content = (message as any).message?.content;
        if (Array.isArray(content)) {
          for (const block of content) {
            if (block.type === "tool_result") {
              const pending = pendingTools.get(block.tool_use_id);
              if (pending) {
                pendingTools.delete(block.tool_use_id);
                const ms = Date.now() - pending.startMs;
                const result = extractResultContent(block.content);
                toolUses.push({
                  id: block.tool_use_id,
                  name: pending.name,
                  input: pending.input,
                  result,
                  isError: !!block.is_error,
                  turn: pending.turn,
                  ms,
                });
              }
            }
          }
        }
      }

      if (message.type === "result" && message.subtype === "success") {
        finalText = message.result ?? "";
      }
    }

    // Flush any tool_use blocks that never got a matching tool_result.
    for (const [id, pending] of pendingTools) {
      toolUses.push({
        id,
        name: pending.name,
        input: pending.input,
        result: undefined,
        isError: false,
        turn: pending.turn,
        ms: Date.now() - pending.startMs,
      });
    }

    // Gather modified tracked files and new untracked files.
    const changedTracked = (() => {
      try {
        return execFileSync("git", ["diff", "--name-only"], { cwd, encoding: "utf-8" })
          .split("\n")
          .filter(Boolean);
      } catch {
        return [] as string[];
      }
    })();

    const newUntracked = (() => {
      try {
        return execFileSync("git", ["ls-files", "--others", "--exclude-standard"], { cwd, encoding: "utf-8" })
          .split("\n")
          .filter(Boolean);
      } catch {
        return [] as string[];
      }
    })();

    const filesChanged = [...new Set([...changedTracked, ...newUntracked])];

    // Build the diff. Use git add -N for new files so they appear in the diff output.
    const diff = (() => {
      try {
        if (newUntracked.length > 0) {
          execFileSync("git", ["add", "-N", ...newUntracked], { cwd, stdio: "pipe" });
        }
        return execFileSync("git", ["diff"], { cwd, encoding: "utf-8" });
      } catch {
        return "";
      }
    })();

    return {
      filesChanged,
      diff,
      text: finalText,
      turnsUsed,
      toolUses,
      thinkingBlocks,
      textBlocks,
    };
  }
}

/**
 * Build a concise one-line summary of a tool call for console logging.
 * File paths are always shown (safety surface). Commands truncated at 200 chars.
 */
function formatToolSummary(name: string, input: unknown): string {
  const inp = (input ?? {}) as Record<string, unknown>;
  switch (name) {
    case "Edit":
      return `${inp.file_path ?? ""}, -${String(inp.old_string ?? "").length}/${String(inp.new_string ?? "").length} chars`;
    case "Write":
      return `${inp.file_path ?? ""}, ${String(inp.content ?? "").length} chars`;
    case "Read":
      return String(inp.file_path ?? "");
    case "Bash":
      return `$ ${String(inp.command ?? "").slice(0, 200)}`;
    case "Glob":
      return `${inp.pattern ?? ""} in ${inp.path ?? "."}`;
    case "Grep":
      return `${inp.pattern ?? ""} in ${inp.path ?? "."}`;
    default:
      return JSON.stringify(input).slice(0, 80);
  }
}

/**
 * Extract the full text content from a tool_result content block.
 * content may be a string or an array of content blocks.
 * Never truncated — per docs/LOGGING.md, the file stream gets full content.
 */
function extractResultContent(content: unknown): string | undefined {
  if (content == null) return undefined;
  if (typeof content === "string") {
    return content;
  }
  if (Array.isArray(content)) {
    const text = content
      .filter((b: any) => b.type === "text")
      .map((b: any) => b.text ?? "")
      .join("");
    return text || undefined;
  }
  return undefined;
}
