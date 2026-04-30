import { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent } from "./Provider";

export interface OpenCodeConfig {
  baseUrl?: string;
}

export class OpenCodeProvider implements LLMProvider {
  readonly name = "opencode";
  private baseUrl: string;
  private client: any = null;
  private sessionID: string | null = null;

  constructor(config: OpenCodeConfig = {}) {
    this.baseUrl = config.baseUrl || process.env.OPENCODE_URL || "http://localhost:4096";
    console.log(`[llm:${this.name}] Configured: ${this.baseUrl}`);
  }

  async complete(prompt: string, options: LLMRequestOptions): Promise<LLMResponse> {
    console.log(`[llm:${this.name}] Sending ${prompt.length} chars (model managed by OpenCode server)`);
    const startTime = Date.now();

    try {
      const client = await this.ensureClient();
      const sessionID = await this.ensureSession(client);
      const eventStream = await client.event.subscribe();
      let fullText = "";
      let done = false;

      const promptPromise = client.session.prompt({
        path: { id: sessionID },
        body: {
          parts: [{ type: "text", text: `${options.systemPrompt}\n\n${prompt}` }],
        },
      });

      const eventPromise = (async () => {
        try {
          for await (const event of eventStream) {
            if (done) break;
            if (event.type === "message.part.updated") {
              const part = event.properties?.part;
              if (part?.type === "text" && part.text) {
                fullText = part.text;
                process.stdout.write(".");
              }
            }
            if (event.type === "session.idle") break;
            if (event.type === "session.error") {
              console.log(`\n[llm:${this.name}] Session error: ${event.properties?.error}`);
              break;
            }
          }
        } catch (e: any) {
          console.log(`[llm:${this.name}] Event stream ended: ${e?.message?.slice(0, 60) || "closed"}`);
        }
      })();

      const result = await promptPromise;
      done = true;

      try { eventStream.controller?.abort(); } catch (e: any) {
        console.log(`[llm:${this.name}] Abort cleanup: ${e?.message?.slice(0, 40) || "ok"}`);
      }

      const text = this.extractText(result) || fullText;
      const elapsed = Date.now() - startTime;
      console.log(`\n[llm:${this.name}] Response in ${this.formatDuration(elapsed)} (${text.length} chars)`);
      return { text };
    } catch (err: any) {
      throw new Error(`[llm:${this.name}] complete() failed: ${err?.message || err}`);
    }
  }

  async *stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent> {
    try {
      const client = await this.ensureClient();
      const sessionID = await this.ensureSession(client);
      const eventStream = await client.event.subscribe();
      let done = false;

      const promptPromise = client.session.prompt({
        path: { id: sessionID },
        body: {
          parts: [{ type: "text", text: `${options.systemPrompt}\n\n${prompt}` }],
        },
      });

      try {
        for await (const event of eventStream) {
          if (done) break;
          if (event.type === "message.part.updated") {
            const part = event.properties?.part;
            if (part?.type === "text" && part.text) {
              yield { type: "text_delta", text: part.text };
            }
          }
          if (event.type === "session.idle" || event.type === "session.error") break;
        }
      } catch (e: any) {
        console.log(`[llm:${this.name}] Stream ended: ${e?.message?.slice(0, 60) || "closed"}`);
      }

      const result = await promptPromise;
      done = true;
      try { eventStream.controller?.abort(); } catch (e: any) {
        console.log(`[llm:${this.name}] Abort cleanup: ${e?.message?.slice(0, 40) || "ok"}`);
      }

      const text = this.extractText(result);
      yield { type: "done", text };
    } catch (err: any) {
      throw new Error(`[llm:${this.name}] stream() failed: ${err?.message || err}`);
    }
  }

  private async ensureClient(): Promise<any> {
    if (this.client) return this.client;
    const mod = await (Function('return import("@opencode-ai/sdk")')() as Promise<any>);
    this.client = mod.createOpencodeClient({ baseUrl: this.baseUrl, timeout: 300000 });
    console.log(`[llm:${this.name}] Client created for ${this.baseUrl}`);
    return this.client;
  }

  private async ensureSession(client: any): Promise<string> {
    if (this.sessionID) return this.sessionID;
    const result = await client.session.create();
    this.sessionID = result.data?.id;
    if (!this.sessionID) {
      throw new Error(`[llm:${this.name}] Failed to create session: ${JSON.stringify(result).slice(0, 200)}`);
    }
    console.log(`[llm:${this.name}] Session created: ${this.sessionID}`);
    return this.sessionID;
  }

  private extractText(result: any): string {
    const data = result?.data;
    if (!data) return "";
    if (data.parts) {
      return data.parts.filter((p: any) => p.type === "text").map((p: any) => p.text).join("");
    }
    if (data.content) {
      if (typeof data.content === "string") return data.content;
      if (Array.isArray(data.content)) {
        return data.content.filter((b: any) => b.type === "text").map((b: any) => b.text).join("");
      }
    }
    return "";
  }

  private formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s`;
    return `${Math.floor(s / 60)}m ${s % 60}s`;
  }
}
