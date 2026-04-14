import { query } from "@anthropic-ai/claude-agent-sdk";
import { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent } from "./Provider";

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
}
