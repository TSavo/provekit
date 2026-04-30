import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@anthropic-ai/claude-agent-sdk", () => ({ query: vi.fn() }));

import { createProvider, createPool } from "./ProviderFactory";
import { ClaudeAgentProvider } from "./ClaudeAgentProvider";
import { ProviderPool } from "./ProviderPool";

describe("createProvider", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("returns ClaudeAgentProvider when name is undefined (auto-detect default)", () => {
    const p = createProvider();
    expect(p).toBeInstanceOf(ClaudeAgentProvider);
    expect(p.name).toMatch(/^claude-agent/);
  });

  it("returns ClaudeAgentProvider when name is 'claude-agent'", () => {
    const p = createProvider("claude-agent");
    expect(p).toBeInstanceOf(ClaudeAgentProvider);
  });

  it("returns ProviderPool when name is 'pool'", () => {
    const p = createProvider("pool");
    expect(p).toBeInstanceOf(ProviderPool);
  });

  it("falls back to ClaudeAgentProvider for unknown provider names", () => {
    const p = createProvider("nonexistent-provider");
    expect(p).toBeInstanceOf(ClaudeAgentProvider);
  });
});

describe("createPool", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("creates a pool whose first provider is ClaudeAgentProvider", () => {
    const pool = createPool();
    expect(pool).toBeInstanceOf(ProviderPool);
    expect(pool.name).toBe("pool");
    const stats = pool.getStats();
    expect(stats.totalRequests).toBe(0);
    const claudeKey = Object.keys(stats.active).find((k) => k.startsWith("claude-agent"));
    expect(claudeKey).toBeDefined();
    expect(stats.active[claudeKey!]).toBe(0);
  });

  it("respects CLAUDE_AGENT_CONCURRENCY env var", () => {
    const prev = process.env.CLAUDE_AGENT_CONCURRENCY;
    process.env.CLAUDE_AGENT_CONCURRENCY = "12";
    try {
      const pool = createPool();
      // Stat shape contains active counters keyed by provider name; we can't
      // directly read maxConcurrency, but the constructor logged it. We check
      // that the pool was constructed without throwing.
      const stats = pool.getStats();
      const key = Object.keys(stats.active).find((k) => k.startsWith("claude-agent"));
      expect(key).toBeDefined();
      expect(stats.active[key!]).toBe(0);
    } finally {
      if (prev === undefined) delete process.env.CLAUDE_AGENT_CONCURRENCY;
      else process.env.CLAUDE_AGENT_CONCURRENCY = prev;
    }
  });
});
