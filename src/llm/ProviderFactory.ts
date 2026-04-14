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

  if (process.env.OPENCODE_URL) {
    providers.push({
      provider: new OpenCodeProvider(),
      maxConcurrency: parseInt(process.env.OPENCODE_CONCURRENCY || "5", 10),
      priority: 0,
    });
  }

  if (process.env.OPENROUTER_API_KEY) {
    providers.push({
      provider: new OpenRouterProvider(),
      maxConcurrency: parseInt(process.env.OPENROUTER_CONCURRENCY || "3", 10),
      priority: 1,
    });
  }

  if (process.env.OPENAI_API_KEY) {
    providers.push({
      provider: new OpenAIProvider(),
      maxConcurrency: parseInt(process.env.OPENAI_CONCURRENCY || "3", 10),
      priority: 2,
    });
  }

  providers.push({
    provider: new ClaudeAgentProvider(),
    maxConcurrency: parseInt(process.env.CLAUDE_AGENT_CONCURRENCY || "5", 10),
    priority: 5,
  });


  return new ProviderPool(providers);
}

function detectProvider(): ProviderName {
  const available: string[] = [];
  if (process.env.OPENCODE_URL) available.push("opencode");
  if (process.env.OPENROUTER_API_KEY) available.push("openrouter");
  if (process.env.OPENAI_API_KEY) available.push("openai");

  if (available.length > 0) return "pool";
  return "claude-agent";
}
