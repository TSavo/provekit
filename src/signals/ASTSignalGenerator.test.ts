import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ASTSignalGenerator } from "./ASTSignalGenerator";
import { parseFile } from "../parser";

describe("ASTSignalGenerator", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("flags branching: if/else, ternary, switch, loops", () => {
    const src = `
function branchy(x: number): string {
  if (x > 0) {} else {}
  for (let i = 0; i < 10; i++) {}
  while (x > 0) { x = x - 1; }
  switch (x) { case 1: break; }
  return x > 0 ? "pos" : "neg";
}
`.trim();
    const tree = parseFile(src);
    const sigs = new ASTSignalGenerator().findSignals("b.ts", src, tree);
    const types = sigs.map((s) => s.type);
    expect(types).toContain("ast:branch");
    expect(types).toContain("ast:loop");
  });

  it("flags arithmetic on parameters", () => {
    const src = `
function divide(a: number, b: number): number {
  return a / b;
}
`.trim();
    const tree = parseFile(src);
    const sigs = new ASTSignalGenerator().findSignals("d.ts", src, tree);
    const arith = sigs.filter((s) => s.type === "ast:arithmetic");
    expect(arith.length).toBeGreaterThanOrEqual(1);
    expect(arith[0].text).toContain("division-by-zero");
  });

  it("flags dangerous calls like execSync", () => {
    const src = `
import { execSync } from "child_process";
function run(cmd: string): void {
  execSync(cmd);
}
`.trim();
    const tree = parseFile(src);
    const sigs = new ASTSignalGenerator().findSignals("r.ts", src, tree);
    const dangerous = sigs.filter((s) => s.type === "ast:dangerous-call");
    expect(dangerous.length).toBeGreaterThanOrEqual(1);
  });

  it("flags empty catch blocks distinctly from non-empty", () => {
    const src = `
function silentFail(): void {
  try { JSON.parse("x"); } catch (e) {}
}
function loudFail(): void {
  try { JSON.parse("x"); } catch (e) { console.log(e); }
}
`.trim();
    const tree = parseFile(src);
    const sigs = new ASTSignalGenerator().findSignals("c.ts", src, tree);
    const errs = sigs.filter((s) => s.type === "ast:error-handling");
    expect(errs.length).toBe(2);
    expect(errs.some((s) => s.text.includes("silently swallowed"))).toBe(true);
  });

  it("returns no signals for trivial functions (no branches, no risky ops)", () => {
    const src = `function add(a: number, b: number): number { return 0; }`;
    const tree = parseFile(src);
    const sigs = new ASTSignalGenerator().findSignals("a.ts", src, tree);
    // The body is a return-literal, no signals. (Arithmetic on params would
    // trigger one — `0` is a literal, no param ref.)
    expect(sigs).toEqual([]);
  });

  it("populates pathConditions from enclosing if branches", () => {
    const src = `
function guarded(x: number): number {
  if (x > 0) {
    return x / 2;
  }
  return 0;
}
`.trim();
    const tree = parseFile(src);
    const sigs = new ASTSignalGenerator().findSignals("g.ts", src, tree);
    const arith = sigs.filter((s) => s.type === "ast:arithmetic");
    expect(arith[0]?.pathConditions.length).toBe(1);
    expect(arith[0]!.pathConditions[0]).toMatch(/x\s*>\s*0/);
  });
});
