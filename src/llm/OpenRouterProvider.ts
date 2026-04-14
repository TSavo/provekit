import { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent } from "./Provider";
import https from "https";

export interface OpenRouterConfig {
  apiKey?: string;
  defaultModel?: string;
}

export class OpenRouterProvider implements LLMProvider {
  readonly name = "openrouter";
  private apiKey: string;
  private defaultModel: string;

  constructor(config: OpenRouterConfig = {}) {
    this.apiKey = config.apiKey || process.env.OPENROUTER_API_KEY || "";
    this.defaultModel = config.defaultModel || process.env.OPENROUTER_MODEL || "openai/gpt-oss-120b:free";
    if (!this.apiKey) {
      console.log(`[llm:${this.name}] WARNING: No API key. Set OPENROUTER_API_KEY.`);
    } else {
      console.log(`[llm:${this.name}] Configured: model=${this.defaultModel}`);
    }
  }

  async complete(prompt: string, options: LLMRequestOptions): Promise<LLMResponse> {
    const model = this.defaultModel;
    console.log(`[llm:${this.name}] Sending ${prompt.length} chars to ${model}`);
    const startTime = Date.now();

    const body = JSON.stringify({
      model,
      messages: [
        { role: "system", content: options.systemPrompt },
        { role: "user", content: prompt },
      ],
    });

    const text = await this.post(body);
    console.log(`[llm:${this.name}] Response received in ${Date.now() - startTime}ms (${text.length} chars)`);
    return { text };
  }

  async *stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent> {
    const response = await this.complete(prompt, options);
    yield { type: "done", text: response.text };
  }

  private post(body: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: "openrouter.ai",
          path: "/api/v1/chat/completions",
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Authorization": `Bearer ${this.apiKey}`,
            "HTTP-Referer": "https://neurallog.app",
            "X-Title": "neurallog",
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
              reject(new Error(`OpenRouter: failed to parse response: ${data.slice(0, 200)}`));
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
