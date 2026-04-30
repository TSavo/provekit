import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { FunctionNameSignalGenerator } from "./FunctionNameSignalGenerator";
import { parseFile } from "../parser";

describe("FunctionNameSignalGenerator", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("emits a signal for sanitize-prefixed functions", () => {
    const src = "function sanitizeInput(s: string): string { return s; }\n";
    const tree = parseFile(src);
    const sigs = new FunctionNameSignalGenerator().findSignals("x.ts", src, tree);
    expect(sigs).toHaveLength(1);
    expect(sigs[0].type).toBe("name:sanitization");
    expect(sigs[0].text).toContain("sanitizeInput");
    expect(sigs[0].text).toContain("must not contain dangerous characters");
    expect(sigs[0].functionName).toBe("sanitizeInput");
    expect(sigs[0].parameters).toEqual([{ name: "s", type: "string" }]);
    expect(sigs[0].returnType).toBe("string");
  });

  it("emits a signal for validate/ensure/verify pattern names", () => {
    const src = `
      function validateEmail(e: string): boolean { return true; }
      function ensureNotNull(v: any): void {}
      function verifyToken(t: string): boolean { return true; }
    `;
    const tree = parseFile(src);
    const sigs = new FunctionNameSignalGenerator().findSignals("x.ts", src, tree);
    const types = sigs.map((s) => s.type);
    expect(types).toContain("name:validation");
    expect(types).toContain("name:guarantee");
    expect(types).toContain("name:verification");
  });

  it("returns no signals when no function names match patterns", () => {
    const src = "function doStuff(x: number): number { return x + 1; }";
    const tree = parseFile(src);
    const sigs = new FunctionNameSignalGenerator().findSignals("x.ts", src, tree);
    expect(sigs).toEqual([]);
  });

  it("matches arrow functions assigned to variables", () => {
    const src = "const isValid = (v: number): boolean => v > 0;\n";
    const tree = parseFile(src);
    const sigs = new FunctionNameSignalGenerator().findSignals("x.ts", src, tree);
    expect(sigs.length).toBeGreaterThanOrEqual(1);
    // isValid matches /^isValid/i first (validation), since iteration starts there.
    const types = sigs.map((s) => s.type);
    expect(types[0]).toMatch(/^name:/);
  });
});
