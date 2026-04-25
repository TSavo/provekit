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

export interface AgentRequestOptions {
  /** Working directory the agent operates in. Agent tools are confined here. */
  cwd: string;
  /** Tools to expose. Default: ["Read", "Edit", "Write", "Bash", "Glob", "Grep"]. */
  allowedTools?: string[];
  /** Model tier. Default depends on provider. */
  model?: "haiku" | "sonnet" | "opus";
  /** Bound turns to prevent runaway. Default 20. */
  maxTurns?: number;
  /** System prompt override. */
  systemPrompt?: string;
}

export interface ToolUseRecord {
  /** Tool call ID from the SDK. */
  id: string;
  /** Tool name: Read, Edit, Write, Bash, Glob, Grep, etc. */
  name: string;
  /** Full input parameters as received from the SDK. */
  input: unknown;
  /** First 500 chars of the tool result (or error text). */
  resultPreview?: string;
  /** Whether the tool result carried is_error=true. */
  isError: boolean;
  /** Turn index (incremented on each assistant message). */
  turn: number;
  /** Wall-clock duration from tool_use emission to tool_result receipt, in ms. */
  ms: number;
}

export interface AgentResult {
  /** Files changed (relative to cwd), from git diff --name-only. */
  filesChanged: string[];
  /** Full git diff output (for audit trail). */
  diff: string;
  /** Final text response from the agent (summary/rationale). */
  text: string;
  /** Number of turns consumed. */
  turnsUsed: number;
  /** All tool calls made during the agent run, in order. */
  toolUses: ToolUseRecord[];
}

export interface LLMProvider {
  readonly name: string;
  complete(prompt: string, options: LLMRequestOptions): Promise<LLMResponse>;
  stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent>;
  // NEW — optional for backward compat with existing providers
  agent?(prompt: string, options: AgentRequestOptions): Promise<AgentResult>;
}
