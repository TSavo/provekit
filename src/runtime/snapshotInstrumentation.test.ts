import { describe, it, expect } from "vitest";
import { instrumentForSnapshot } from "./snapshotInstrumentation.js";

describe("instrumentForSnapshot", () => {
  it("inserts a snapshot call at the signal line capturing named locals", () => {
    const source = `
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("result", q);
  return q;
}
    `.trim();
    const result = instrumentForSnapshot(source, { signalLine: 3, captureNames: ["a", "b", "q"] });
    expect(result).toContain("__provekit_snapshot__");
    expect(result).toContain('"divide"');
    expect(result).toMatch(/__provekit_snapshot__\(\s*"divide"\s*,\s*3\s*,\s*\{\s*a\s*,\s*b\s*,\s*q\s*\}\s*\)/);
    expect(result).toContain('console.log("result", q)');
  });

  it("handles functions with expression-body arrow", () => {
    const source = `export const f = (x: number) => x + 1;`;
    const result = instrumentForSnapshot(source, { signalLine: 1, captureNames: ["x"] });
    expect(result).toContain("__provekit_snapshot__");
    expect(result).toContain("return");
  });

  it("returns source unchanged if signalLine is outside any function", () => {
    const source = `const x = 1;\nconst y = 2;\n`;
    const result = instrumentForSnapshot(source, { signalLine: 1, captureNames: ["x"] });
    expect(result).toBe(source);
  });
});
