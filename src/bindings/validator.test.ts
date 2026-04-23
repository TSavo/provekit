import { describe, it, expect } from "vitest";
import { validateBindings } from "./validator.js";

describe("validateBindings", () => {
  const source = `
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("q", q);
  return q;
}
  `.trim();

  it("accepts bindings whose source_line and source_expr are present", () => {
    const res = validateBindings(source, [
      { smtConstant: "a", sourceLine: 1, sourceExpr: "a", sort: "Real" },
      { smtConstant: "b", sourceLine: 1, sourceExpr: "b", sort: "Real" },
      { smtConstant: "q", sourceLine: 2, sourceExpr: "a / b", sort: "Real" },
    ]);
    expect(res.valid).toHaveLength(3);
    expect(res.invalid).toHaveLength(0);
  });

  it("rejects a binding whose source_line is out of range", () => {
    const res = validateBindings(source, [
      { smtConstant: "x", sourceLine: 99, sourceExpr: "x", sort: "Real" },
    ]);
    expect(res.invalid).toHaveLength(1);
    expect(res.invalid[0].reason).toMatch(/line 99 out of range/);
  });

  it("rejects a binding whose source_expr is absent from the declared line", () => {
    const res = validateBindings(source, [
      { smtConstant: "ghost", sourceLine: 2, sourceExpr: "nonexistent_expr", sort: "Real" },
    ]);
    expect(res.invalid).toHaveLength(1);
    expect(res.invalid[0].reason).toMatch(/source_expr.*not found/);
  });
});
