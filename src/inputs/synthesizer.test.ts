import { describe, it, expect } from "vitest";
import { synthesizeInputs, materializeZ3Value } from "./synthesizer.js";
import type { SmtBinding } from "../contracts.js";
import type { Z3Value } from "../z3/modelParser.js";

const divideSource = `
export function divide(a: number, b: number): number {
  const q = a / b;
  return q;
}
`;

const divideBindings: SmtBinding[] = [
  { smt_constant: "a", source_line: 2, source_expr: "a", sort: "Int" },
  { smt_constant: "b", source_line: 2, source_expr: "b", sort: "Int" },
];

describe("synthesizeInputs", () => {
  it("case 1: both params constrained — divide(a,b) with a=0, b=0", () => {
    const model = new Map<string, Z3Value>([
      ["a", { sort: "Int", value: 0n }],
      ["b", { sort: "Int", value: 0n }],
    ]);
    const result = synthesizeInputs({
      functionSource: divideSource,
      functionName: "divide",
      bindings: divideBindings,
      z3Model: model,
    });
    expect(result).toEqual({ a: 0, b: 0 });
  });

  it("case 2: one param constrained, other defaults by type", () => {
    const source = `
function foo(x: number, y: number): number {
  return x + y;
}
`;
    const bindings: SmtBinding[] = [
      { smt_constant: "x_var", source_line: 2, source_expr: "x", sort: "Int" },
    ];
    const model = new Map<string, Z3Value>([
      ["x_var", { sort: "Int", value: 5n }],
    ]);
    const result = synthesizeInputs({
      functionSource: source,
      functionName: "foo",
      bindings,
      z3Model: model,
    });
    expect(result).toEqual({ x: 5, y: 0 });
  });

  it("case 3: Real nan witness → NaN", () => {
    const model = new Map<string, Z3Value>([
      ["a", { sort: "Real", value: "nan" }],
      ["b", { sort: "Int", value: 1n }],
    ]);
    const result = synthesizeInputs({
      functionSource: divideSource,
      functionName: "divide",
      bindings: divideBindings,
      z3Model: model,
    });
    expect(Number.isNaN(result["a"])).toBe(true);
    expect(result["b"]).toBe(1);
  });

  it("case 4: Real +infinity witness → Infinity", () => {
    const model = new Map<string, Z3Value>([
      ["a", { sort: "Real", value: "+infinity" }],
      ["b", { sort: "Int", value: 1n }],
    ]);
    const result = synthesizeInputs({
      functionSource: divideSource,
      functionName: "divide",
      bindings: divideBindings,
      z3Model: model,
    });
    expect(result["a"]).toBe(Infinity);
  });

  it("case 5a: Int safe-integer → number", () => {
    const v: Z3Value = { sort: "Int", value: 42n };
    expect(materializeZ3Value(v)).toBe(42);
    expect(typeof materializeZ3Value(v)).toBe("number");
  });

  it("case 5b: Int huge (2n ** 100n) → bigint", () => {
    const big = 2n ** 100n;
    const v: Z3Value = { sort: "Int", value: big };
    expect(materializeZ3Value(v)).toBe(big);
    expect(typeof materializeZ3Value(v)).toBe("bigint");
  });

  it("case 6: Bool witness → boolean", () => {
    const source = `
function check(flag: boolean): boolean {
  return !flag;
}
`;
    const bindings: SmtBinding[] = [
      { smt_constant: "flag_smt", source_line: 2, source_expr: "flag", sort: "Bool" },
    ];
    const model = new Map<string, Z3Value>([
      ["flag_smt", { sort: "Bool", value: true }],
    ]);
    const result = synthesizeInputs({
      functionSource: source,
      functionName: "check",
      bindings,
      z3Model: model,
    });
    expect(result).toEqual({ flag: true });
  });

  it("case 7: function with no params → empty object", () => {
    const source = `
function noParams(): number {
  return 42;
}
`;
    const result = synthesizeInputs({
      functionSource: source,
      functionName: "noParams",
      bindings: [],
      z3Model: new Map(),
    });
    expect(result).toEqual({});
  });

  it("case 8: arrow function via const → finds params correctly", () => {
    const source = `
const f = (x: number) => x + 1;
`;
    const bindings: SmtBinding[] = [
      { smt_constant: "x_smt", source_line: 2, source_expr: "x", sort: "Int" },
    ];
    const model = new Map<string, Z3Value>([
      ["x_smt", { sort: "Int", value: 7n }],
    ]);
    const result = synthesizeInputs({
      functionSource: source,
      functionName: "f",
      bindings,
      z3Model: model,
    });
    expect(result).toEqual({ x: 7 });
  });

  it("case 9: binding source_expr with whitespace still matches param", () => {
    const source = `
function add(a: number, b: number): number {
  return a + b;
}
`;
    const bindings: SmtBinding[] = [
      { smt_constant: "a_smt", source_line: 2, source_expr: " a ", sort: "Int" },
      { smt_constant: "b_smt", source_line: 2, source_expr: " b ", sort: "Int" },
    ];
    const model = new Map<string, Z3Value>([
      ["a_smt", { sort: "Int", value: 3n }],
      ["b_smt", { sort: "Int", value: 4n }],
    ]);
    const result = synthesizeInputs({
      functionSource: source,
      functionName: "add",
      bindings,
      z3Model: model,
    });
    expect(result).toEqual({ a: 3, b: 4 });
  });

  it("Real number → JS number", () => {
    const v: Z3Value = { sort: "Real", value: 3.14 };
    expect(materializeZ3Value(v)).toBe(3.14);
  });

  it("Real div_by_zero → NaN", () => {
    const v: Z3Value = { sort: "Real", value: "div_by_zero" };
    expect(Number.isNaN(materializeZ3Value(v))).toBe(true);
  });

  it("Real -infinity → -Infinity", () => {
    const v: Z3Value = { sort: "Real", value: "-infinity" };
    expect(materializeZ3Value(v)).toBe(-Infinity);
  });

  it("String witness → string", () => {
    const v: Z3Value = { sort: "String", value: "hello" };
    expect(materializeZ3Value(v)).toBe("hello");
  });

  it("Other → raw string", () => {
    const v: Z3Value = { sort: "Other", raw: "(some-expr)" };
    expect(materializeZ3Value(v)).toBe("(some-expr)");
  });
});
