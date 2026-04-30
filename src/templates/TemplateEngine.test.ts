/**
 * TemplateEngine surface smoke tests. Uses the real .provekit/principles/
 * dir from the project root (same as the existing bindings test does).
 * Walks up from this test file's directory, the same way the bindings test
 * does, but without import.meta (which breaks under commonjs tsconfig).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { dirname, join } from "path";
import { existsSync } from "fs";
import { parseFile } from "../parser";
import { TemplateEngine } from "./TemplateEngine";

function findProjectRoot(): string {
  let dir = __dirname;
  while (dir !== dirname(dir)) {
    if (existsSync(join(dir, ".provekit", "principles"))) return dir;
    dir = dirname(dir);
  }
  throw new Error("could not locate project root with .provekit/principles/");
}

const PROJECT_ROOT = findProjectRoot();

describe("TemplateEngine.generateProofs", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("returns an empty array when fnNode has no body", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    // Use a top-level statement that does NOT have a function body — the
    // generator early-returns when childForFieldName('body') is null.
    const tree = parseFile("const x = 1;");
    const root = tree.rootNode;
    expect(engine.generateProofs(root, "noop", "x.ts")).toEqual([]);
  });

  it("returns no results for a function with no risky operations", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const src = "function nop(): void { return; }";
    const tree = parseFile(src);
    const fnNode = tree.rootNode.children.find((c) => c.type === "function_declaration")!;
    expect(fnNode).toBeTruthy();
    const results = engine.generateProofs(fnNode, "nop", "x.ts");
    expect(results).toEqual([]);
  });

  it("returns at least one division-by-zero proof for a divide() function", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const src = `function divide(a: number, b: number): number {
  const q = a / b;
  return q;
}`;
    const tree = parseFile(src);
    const fnNode = tree.rootNode.children.find((c) => c.type === "function_declaration")!;
    expect(fnNode).toBeTruthy();
    const results = engine.generateProofs(fnNode, "divide", "test.ts");
    const dz = results.find((r) => r.principle === "division-by-zero");
    expect(dz).toBeDefined();
    expect(dz!.smt2.length).toBeGreaterThan(0);
    expect(dz!.signalLine).toBe(2);
  });
});
