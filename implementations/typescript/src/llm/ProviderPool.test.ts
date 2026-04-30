import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ProviderPool } from "./ProviderPool";
import type { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent } from "./Provider";

class StubProvider implements LLMProvider {
  readonly name: string;
  public callCount = 0;
  constructor(
    name: string,
    private behavior: { kind: "ok"; text: string } | { kind: "error"; message: string },
  ) {
    this.name = name;
  }
  async complete(_p: string, _o: LLMRequestOptions): Promise<LLMResponse> {
    this.callCount++;
    if (this.behavior.kind === "error") {
      throw new Error(this.behavior.message);
    }
    return { text: this.behavior.text };
  }
  async *stream(): AsyncIterable<LLMStreamEvent> {
    yield { type: "done", text: "x" };
  }
}

describe("ProviderPool.complete", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  const opts: LLMRequestOptions = { model: "test", systemPrompt: "" };

  it("routes the first request to the highest-priority provider", async () => {
    const a = new StubProvider("a", { kind: "ok", text: "from-a" });
    const b = new StubProvider("b", { kind: "ok", text: "from-b" });
    const pool = new ProviderPool([
      { provider: a, maxConcurrency: 5, priority: 0 },
      { provider: b, maxConcurrency: 5, priority: 1 },
    ]);

    const r = await pool.complete("hi", opts);
    expect(r.text).toBe("from-a");
    expect(a.callCount).toBe(1);
    expect(b.callCount).toBe(0);
  });

  it("fails over to the next provider when the first throws", async () => {
    const a = new StubProvider("a", { kind: "error", message: "rate limit" });
    const b = new StubProvider("b", { kind: "ok", text: "from-b" });
    const pool = new ProviderPool([
      { provider: a, maxConcurrency: 5, priority: 0 },
      { provider: b, maxConcurrency: 5, priority: 1 },
    ]);

    const r = await pool.complete("hi", opts);
    expect(r.text).toBe("from-b");
    expect(pool.getStats().totalFailovers).toBe(1);
  });

  it("releases the slot back to 0 after a successful call", async () => {
    const a = new StubProvider("a", { kind: "ok", text: "ok" });
    const pool = new ProviderPool([{ provider: a, maxConcurrency: 1, priority: 0 }]);
    await pool.complete("hi", opts);
    expect(pool.getStats().active["a"]).toBe(0);
  });

  it("getStats reflects total requests counted at start of complete", async () => {
    const a = new StubProvider("a", { kind: "ok", text: "ok" });
    const pool = new ProviderPool([{ provider: a, maxConcurrency: 5, priority: 0 }]);
    await pool.complete("one", opts);
    await pool.complete("two", opts);
    expect(pool.getStats().totalRequests).toBe(2);
  });
});
