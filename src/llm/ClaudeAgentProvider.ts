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
        // Same permission posture as agent(): bypass everything, every tool
        // available, no turn cap. Without this the SDK falls back to its
        // restrictive default and prompts that say "use the Write tool"
        // produce "I need write permission" prose responses.
        allowedTools: [".*"],
        permissionMode: "bypassPermissions",
        allowDangerouslySkipPermissions: true,
        maxTurns: 1000,
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
        allowedTools: [".*"],
        permissionMode: "bypassPermissions",
        allowDangerouslySkipPermissions: true,
        maxTurns: 1000,
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
    // Default: regex wildcard — let the agent use whatever it needs (built-in,
    // MCP, skills). Callers that need a tighter contract (e.g. structured-
    // JSON output via Write) pass an explicit narrower list.
    const allowedTools = options.allowedTools ?? [".*"];
    // 1000 is effectively no cap. Per user directive: never artificially
    // gate agent calls — let the LLM use whatever turns/tools it needs.
    const maxTurns = options.maxTurns ?? 1000;

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

    // PROVEKIT_LLM_VERBOSE=1 dumps every SDK message as it arrives. Use to
    // diagnose hangs (e.g. agent is generating text/thinking with no tool
    // calls vs. genuinely stuck waiting on the API).
    const verbose = process.env.PROVEKIT_LLM_VERBOSE === "1";

    for await (const message of query({
      prompt,
      options: {
        model,
        cwd,
        allowedTools,
        maxTurns,
        systemPrompt: options.systemPrompt,
        includePartialMessages: verbose,
        // bypassPermissions accepts every tool action without prompting.
        // acceptEdits rejected Write to scratch /var/folders paths even
        // with allowedTools=[".*"] — observed across C1 + Investigate
        // failures where the agent responded with "I need write permission"
        // prose instead of writing the JSON contract file. The fix loop's
        // entire output channel relies on the agent writing structured
        // JSON to a known scratch path; partial permission gating turns
        // every stage into a coin flip on prompt obedience.
        //
        // bypassPermissions requires allowDangerouslySkipPermissions=true
        // as an explicit acknowledgement (per SDK type def). Without it,
        // the SDK silently falls back to default permission gating.
        permissionMode: "bypassPermissions",
        allowDangerouslySkipPermissions: true,
        thinking: { type: "enabled", budgetTokens: 4096 },
      },
    })) {
      if (verbose) {
        const mtype = message.type;
        const mstub = mtype === "assistant" || mtype === "user"
          ? `${mtype} content_blocks=${(message as any).message?.content?.length ?? 0}`
          : mtype === "stream_event"
            ? `stream_event ${(message as any).event?.type ?? "?"}`
            : mtype === "result"
              ? `result subtype=${(message as any).subtype}`
              : mtype;
        console.log(`[llm:${this.name}:event] ${mstub}`);
      }
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

    // Gather modified tracked files and new untracked files. The cwd
    // may not be a git repo (e.g. structured-output's temp scratch dir);
    // when git emits "Not a git repository" or other errors we already
    // catch the throw and return empty, but we ALSO need to suppress the
    // child process's stderr so it doesn't leak ~100 lines of git
    // --no-index help text to the parent terminal. Hence stdio's stderr
    // arm pinned to "pipe" everywhere — captures stderr for our own
    // discard; never lets it through to the user.
    const gitStdio: ["pipe", "pipe", "pipe"] = ["pipe", "pipe", "pipe"];
    const changedTracked = (() => {
      try {
        return execFileSync("git", ["diff", "--name-only"], {
          cwd,
          encoding: "utf-8",
          stdio: gitStdio,
        })
          .split("\n")
          .filter(Boolean);
      } catch {
        return [] as string[];
      }
    })();

    const newUntracked = (() => {
      try {
        return execFileSync("git", ["ls-files", "--others", "--exclude-standard"], {
          cwd,
          encoding: "utf-8",
          stdio: gitStdio,
        })
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
          execFileSync("git", ["add", "-N", ...newUntracked], { cwd, stdio: gitStdio });
        }
        return execFileSync("git", ["diff"], {
          cwd,
          encoding: "utf-8",
          stdio: gitStdio,
        });
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
