import { describe, it, expect } from "vitest";
import { parseJsonFromLlm, extractJsonFromText } from "./llmJson.js";

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

describe("extractJsonFromText", () => {
  it("returns parsed object for bare JSON", () => {
    expect(extractJsonFromText('{"a": 1}')).toEqual({ a: 1 });
  });

  it("returns parsed object for fenced JSON", () => {
    expect(extractJsonFromText('```json\n{"b": 2}\n```')).toEqual({ b: 2 });
  });

  it("extracts JSON from prose-prefixed text", () => {
    const text = `I'll write the file. Here it is: {"recovered": true, "value": 42}`;
    expect(extractJsonFromText(text)).toEqual({ recovered: true, value: 42 });
  });

  it("extracts JSON from text with trailing prose", () => {
    const text = `{"primary": "fix"}\n\nI hope this helps!`;
    expect(extractJsonFromText(text)).toEqual({ primary: "fix" });
  });

  it("extracts fenced JSON anywhere in text", () => {
    const text = `Some preamble.\n\nHere's the JSON:\n\`\`\`json\n{"in_fence": true}\n\`\`\`\n\nDone.`;
    expect(extractJsonFromText(text)).toEqual({ in_fence: true });
  });

  it("extracts JSON arrays from text", () => {
    const text = `result: [1, 2, 3]`;
    expect(extractJsonFromText(text)).toEqual([1, 2, 3]);
  });

  it("returns null when no JSON is extractable", () => {
    expect(extractJsonFromText("I encountered an error and couldn't produce a result.")).toBeNull();
  });

  it("returns null on empty string", () => {
    expect(extractJsonFromText("")).toBeNull();
  });

  it("handles nested objects with prose around them", () => {
    const text = `analyzed the code. Output:\n\n{"outer": {"inner": [1, 2]}, "flag": true}\n\nthat's it.`;
    expect(extractJsonFromText(text)).toEqual({ outer: { inner: [1, 2] }, flag: true });
  });
});
