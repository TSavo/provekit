import { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent } from "./Provider";
import https from "https";

export interface OpenAIConfig {
  apiKey?: string;
  baseURL?: string;
}

export class OpenAIProvider implements LLMProvider {
  readonly name = "openai";
  private apiKey: string;
  private hostname: string;
  private basePath: string;

  constructor(config: OpenAIConfig = {}) {
    this.apiKey = config.apiKey || process.env.OPENAI_API_KEY || "";
    const url = new URL(config.baseURL || process.env.OPENAI_BASE_URL || "https://api.openai.com");
    this.hostname = url.hostname;
    this.basePath = url.pathname.replace(/\/$/, "");

    if (!this.apiKey) {
      console.log(`[llm:${this.name}] WARNING: No API key. Set OPENAI_API_KEY.`);
    } else {
      console.log(`[llm:${this.name}] Configured: ${this.hostname}`);
    }
  }

  async complete(prompt: string, options: LLMRequestOptions): Promise<LLMResponse> {
    console.log(`[llm:${this.name}] Sending ${prompt.length} chars to ${options.model}`);
    const startTime = Date.now();

    const body = JSON.stringify({
      model: options.model,
      messages: [
        { role: "system", content: options.systemPrompt },
        { role: "user", content: prompt },
      ],
    });

    try {
      const text = await this.post(body);
      console.log(`[llm:${this.name}] Response received in ${Date.now() - startTime}ms (${text.length} chars)`);
      return { text };
    } catch (err: any) {
      throw new Error(`[llm:${this.name}] complete() failed: ${err?.message || err}`);
    }
  }

  async *stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent> {
    const response = await this.complete(prompt, options);
    yield { type: "done", text: response.text };
  }

  private post(body: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: this.hostname,
          path: `${this.basePath}/v1/chat/completions`,
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Authorization": `Bearer ${this.apiKey}`,
          },
        },
        (res) => {
          let data = "";
          res.on("data", (chunk) => { data += chunk; });
          res.on("end", () => {
            try {
              const parsed = JSON.parse(data);
              const text = parsed.choices?.[0]?.message?.content || "";
              resolve(text);
            } catch {
              reject(new Error(`OpenAI: failed to parse response: ${data.slice(0, 200)}`));
            }
          });
        }
      );
      req.on("error", reject);
      req.write(body);
      req.end();
    });
  }
}
