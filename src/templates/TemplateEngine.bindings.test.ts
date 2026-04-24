import { describe, it, expect } from "vitest";
import { join } from "path";
import { fileURLToPath } from "url";
import { parseFile } from "../parser";
import { TemplateEngine } from "./TemplateEngine";

// The worktree root contains .neurallog/principles/
const PROJECT_ROOT = join(fileURLToPath(import.meta.url), "../../../../..");

const FIXTURE = `function divide(a: number, b: number): number {
  const q = a / b;
  return q;
}`;

describe("TemplateEngine.extractBindings for binary_expression (division-by-zero)", () => {
  it("produces bindings mapping SMT constants to source positions", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(FIXTURE);

    // Find the function_declaration node
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const results = engine.generateProofs(fnNode!, "divide", "test.ts");

    // Filter to division-by-zero
    const dzResult = results.find((r) => r.principle === "division-by-zero");
    expect(dzResult).toBeTruthy();

    const { bindings } = dzResult!;

    // Must have at least one binding
    expect(bindings.length).toBeGreaterThan(0);

    // The denominator binding: smt_constant "b", source_expr "b"
    // `a / b` is on line 2 of the fixture (1-indexed)
    const denomBinding = bindings.find((b) => b.smt_constant === "b");
    expect(denomBinding).toBeTruthy();
    expect(denomBinding!.source_expr).toBe("b");
    expect(denomBinding!.source_line).toBe(2);
    // division-by-zero template declares Int
    expect(denomBinding!.sort).toBe("Int");
  });
});
