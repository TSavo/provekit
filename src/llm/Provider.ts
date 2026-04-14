export interface LLMResponse {
  text: string;
}

export interface LLMStreamEvent {
  type: "text_delta" | "done";
  text?: string;
}

export interface LLMRequestOptions {
  model: string;
  systemPrompt: string;
}

export interface LLMProvider {
  readonly name: string;
  complete(prompt: string, options: LLMRequestOptions): Promise<LLMResponse>;
  stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent>;
}
