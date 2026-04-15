import { LLMProvider } from "./Provider";
import { ClaudeAgentProvider } from "./ClaudeAgentProvider";
import { OpenCodeProvider } from "./OpenCodeProvider";
import { OpenAIProvider } from "./OpenAIProvider";
import { OpenRouterProvider } from "./OpenRouterProvider";
import { ProviderPool, PooledProvider } from "./ProviderPool";

export type ProviderName = "claude-agent" | "opencode" | "openai" | "openrouter" | "pool";

export function createProvider(name?: ProviderName | string): LLMProvider {
  const resolved = name || detectProvider();

  console.log(`[llm] Creating provider: ${resolved}`);

  switch (resolved) {
    case "claude-agent":
      return new ClaudeAgentProvider();
    case "opencode":
      return new OpenCodeProvider();
    case "openai":
      return new OpenAIProvider();
    case "openrouter":
      return new OpenRouterProvider();
    case "pool":
      return createPool();
    default:
      console.log(`[llm] Unknown provider "${resolved}", falling back to claude-agent`);
      return new ClaudeAgentProvider();
  }
}

export function createPool(): ProviderPool {
  const providers: PooledProvider[] = [];

  providers.push({
    provider: new ClaudeAgentProvider(),
    maxConcurrency: parseInt(process.env.CLAUDE_AGENT_CONCURRENCY || "5", 10),
    priority: 0,
  });


  return new ProviderPool(providers);
}

function detectProvider(): ProviderName {
  return "claude-agent";
}
