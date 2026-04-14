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

    const client = await this.ensureClient();
    const sessionID = await this.ensureSession(client);

    const result = await client.session.prompt({
      path: { id: sessionID },
      body: {
        parts: [{ type: "text", text: `${options.systemPrompt}\n\n${prompt}` }],
      },
    });

    const text = this.extractText(result);
    console.log(`[llm:${this.name}] Response received in ${Date.now() - startTime}ms (${text.length} chars)`);
    return { text };
  }

  async *stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent> {
    const response = await this.complete(prompt, options);
    yield { type: "done", text: response.text };
  }

  private async ensureClient(): Promise<any> {
    if (this.client) return this.client;
    const mod = await (Function('return import("@opencode-ai/sdk")')() as Promise<any>);
    this.client = mod.createOpencodeClient({ baseUrl: this.baseUrl });
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
    if (!data) return JSON.stringify(result);

    if (data.parts) {
      return data.parts
        .filter((p: any) => p.type === "text")
        .map((p: any) => p.text)
        .join("");
    }

    if (data.content) {
      if (typeof data.content === "string") return data.content;
      if (Array.isArray(data.content)) {
        return data.content
          .filter((b: any) => b.type === "text")
          .map((b: any) => b.text)
          .join("");
      }
    }

    return JSON.stringify(data);
  }
}
