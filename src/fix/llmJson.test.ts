import { describe, it, expect } from "vitest";
import { parseJsonFromLlm } from "./llmJson.js";

describe("parseJsonFromLlm", () => {
  it("parses clean JSON", () => {
    const result = parseJsonFromLlm<{ a: number }>('{"a": 1}');
    expect(result).toEqual({ a: 1 });
  });

  it("strips ```json fence", () => {
    const raw = '```json\n{"primaryLayer": "code_invariant"}\n```';
    const result = parseJsonFromLlm<{ primaryLayer: string }>(raw);
    expect(result.primaryLayer).toBe("code_invariant");
  });

  it("strips plain ``` fence", () => {
    const raw = '```\n{"x": 42}\n```';
    const result = parseJsonFromLlm<{ x: number }>(raw);
    expect(result.x).toBe(42);
  });

  it("strips trailing ``` without language tag", () => {
    const raw = '```\n{"done": true}\n```';
    const result = parseJsonFromLlm<{ done: boolean }>(raw);
    expect(result.done).toBe(true);
  });

  it("parses JSON with leading/trailing whitespace", () => {
    const raw = '  \n  {"key": "value"}  \n  ';
    const result = parseJsonFromLlm<{ key: string }>(raw);
    expect(result.key).toBe("value");
  });

  it("throws informative error on non-JSON", () => {
    const raw = "This is not JSON at all, sorry";
    expect(() => parseJsonFromLlm(raw, "testSite")).toThrowError(/parseJsonFromLlm \[testSite\]/);
  });

  it("error message includes raw response truncated to 500 chars", () => {
    const raw = "x".repeat(600);
    let caught: Error | undefined;
    try {
      parseJsonFromLlm(raw);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught).toBeDefined();
    // Raw response in error should be truncated: the message contains exactly 500 x's
    expect(caught!.message).toContain("x".repeat(500));
    expect(caught!.message).not.toContain("x".repeat(501));
  });
});
