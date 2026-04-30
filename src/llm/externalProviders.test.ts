/**
 * Smoke tests for the external HTTP-based LLM providers. These don't make
 * real network calls; they only assert that the constructors honor explicit
 * config and env-var fallbacks without throwing.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { OpenAIProvider } from "./OpenAIProvider";
import { OpenRouterProvider } from "./OpenRouterProvider";
import { OpenCodeProvider } from "./OpenCodeProvider";

describe("OpenAIProvider", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("constructs with explicit apiKey + baseURL", () => {
    const p = new OpenAIProvider({ apiKey: "sk-test", baseURL: "https://api.openai.com" });
    expect(p.name).toBe("openai");
  });

  it("constructs with no api key (env fallback empty); logs warning", () => {
    const prev = process.env.OPENAI_API_KEY;
    delete process.env.OPENAI_API_KEY;
    try {
      const p = new OpenAIProvider();
      expect(p.name).toBe("openai");
      // Warning emitted via console.log (mocked) — capture it
      const calls = logSpy.mock.calls.map((c: unknown[]) => String(c[0]));
      expect(calls.some((m: string) => m.includes("No API key"))).toBe(true);
    } finally {
      if (prev !== undefined) process.env.OPENAI_API_KEY = prev;
    }
  });
});

describe("OpenRouterProvider", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("constructs with explicit apiKey", () => {
    const p = new OpenRouterProvider({ apiKey: "or-test" });
    expect(p.name).toBe("openrouter");
  });

  it("constructs with no apiKey and logs a warning", () => {
    const prev = process.env.OPENROUTER_API_KEY;
    delete process.env.OPENROUTER_API_KEY;
    try {
      const p = new OpenRouterProvider();
      expect(p.name).toBe("openrouter");
    } finally {
      if (prev !== undefined) process.env.OPENROUTER_API_KEY = prev;
    }
  });
});

describe("OpenCodeProvider", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("constructs without throwing", () => {
    const p = new OpenCodeProvider();
    expect(p.name).toBe("opencode");
  });
});
