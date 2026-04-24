import { query } from "@anthropic-ai/claude-agent-sdk";
import { execFileSync } from "child_process";
import { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent, AgentRequestOptions, AgentResult } from "./Provider";

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
      opus: "claude-opus-4-5",
    };
    const model = options.model ? (modelMap[options.model] ?? options.model) : undefined;

    let turnsUsed = 0;
    let finalText = "";

    for await (const message of query({
      prompt,
      options: {
        model,
        cwd,
        allowedTools,
        maxTurns,
        systemPrompt: options.systemPrompt,
        permissionMode: "acceptEdits",
      },
    })) {
      if (message.type === "assistant") {
        turnsUsed++;
      }
      if (message.type === "result" && message.subtype === "success") {
        finalText = message.result ?? "";
      }
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
    };
  }
}
