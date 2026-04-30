import { describe, it, expect } from "vitest";
import { PropertyTestChecker } from "./PropertyTestChecker";

// Private-method testing via any-cast. The helpers under test are pure
// logic; surfacing them for tests is cheaper than refactoring them into
// standalone modules.
const pct = new PropertyTestChecker(process.cwd()) as any;

describe("isCompatibleWithTsType", () => {
  const cases: [string, unknown, boolean][] = [
    ["number", 5, true],
    ["number", 0, true],
    ["number", -3.14, true],
    ["number", "5", false],
    ["number", true, false],
    ["Number", 5, true],
    ["boolean", true, true],
    ["boolean", false, true],
    ["boolean", 0, false],
    ["string", "hello", true],
    ["string", 5, false],
    ["any", 5, true],
    ["any", "x", true],
    ["any", { a: 1 }, true],
    ["unknown", 5, true],
    ["", 5, true],
    ["number | undefined", 5, true],
    ["number | undefined", undefined, true],
    ["string | null", "x", true],
    ["string | null", null, true],
    ["string | null", 5, false],
    ["Contract[]", 5, false],
    ["Map<string, Contract>", 5, false],
    ["string[]", 0, false],
    ['"literal"', "literal", true],
    ["42", 42, true],
    ["42", 43, true],
  ];
  for (const [tsType, value, expected] of cases) {
    it(`${JSON.stringify(tsType)} vs ${typeof value}:${JSON.stringify(value)} → ${expected}`, () => {
      expect(pct.isCompatibleWithTsType(tsType, value)).toBe(expected);
    });
  }
});

describe("matchParamToModel fuzzy rules", () => {
  it("exact name match", () => {
    expect(pct.matchParamToModel("x", { x: 5 })).toBe(5);
  });

  it("case-fold match", () => {
    expect(pct.matchParamToModel("MyVar", { myvar: 7 })).toBe(7);
  });

  it("underscore-stripped match", () => {
    expect(pct.matchParamToModel("my_var", { myvar: 9 })).toBe(9);
  });

  it("suffix-strip _condition", () => {
    expect(pct.matchParamToModel("value", { value_condition: 11 })).toBe(11);
  });

  it("suffix-strip _guard", () => {
    expect(pct.matchParamToModel("count", { count_guard: 13 })).toBe(13);
  });

  it("substring match (model key contains param)", () => {
    expect(pct.matchParamToModel("bal", { bal_after_transfer: 100 })).toBe(100);
  });

  it("returns undefined when no match", () => {
    expect(pct.matchParamToModel("foo", { bar: 1, baz: 2 })).toBeUndefined();
  });

  it("prefers exact over normalized", () => {
    expect(pct.matchParamToModel("x", { x: 1, X: 2 })).toBe(1);
  });
});

describe("isControlFlowModel", () => {
  it("returns true when all keys are control-flow labels", () => {
    const model = { guard_condition: 1, guard_returns: 0, code_after_reached: 1 };
    expect(pct.isControlFlowModel(model, ["value", "min", "max"])).toBe(true);
  });

  it("returns false when at least one key matches a param", () => {
    const model = { guard_condition: 1, value: 5 };
    expect(pct.isControlFlowModel(model, ["value", "min"])).toBe(false);
  });

  it("returns false for empty model", () => {
    expect(pct.isControlFlowModel({}, ["x"])).toBe(false);
  });

  it("recognizes result_consequent / result_alternate", () => {
    const model = { result_consequent: 1, result_alternate: 2, d: 3 };
    // `d` is parameter-like, not a known control-flow label, so mixed => false
    expect(pct.isControlFlowModel(model, ["d"])).toBe(false);
  });
});

describe("parseZ3Model — sexp forms", () => {
  it("plain integer", () => {
    const text = "(\n  (define-fun x () Int 5)\n)";
    expect(pct.parseZ3Model(text)).toEqual({ x: 5 });
  });

  it("negative integer via (- N)", () => {
    const text = "(\n  (define-fun x () Int (- 7))\n)";
    expect(pct.parseZ3Model(text)).toEqual({ x: -7 });
  });

  it("Real fraction via (/ a b)", () => {
    const text = "(\n  (define-fun x () Real (/ 5 2))\n)";
    const model = pct.parseZ3Model(text);
    expect(model.x).toBeCloseTo(2.5);
  });

  it("negated fraction via (- (/ a b))", () => {
    const text = "(\n  (define-fun x () Real (- (/ 5 2)))\n)";
    const model = pct.parseZ3Model(text);
    expect(model.x).toBeCloseTo(-2.5);
  });

  it("boolean true/false", () => {
    const text = "(\n  (define-fun p () Bool true)\n  (define-fun q () Bool false)\n)";
    expect(pct.parseZ3Model(text)).toEqual({ p: true, q: false });
  });

  it("multiple bindings", () => {
    const text = "(\n  (define-fun a () Int 1)\n  (define-fun b () Int (- 2))\n  (define-fun c () Real (/ 3 4))\n)";
    const model = pct.parseZ3Model(text);
    expect(model.a).toBe(1);
    expect(model.b).toBe(-2);
    expect(model.c).toBeCloseTo(0.75);
  });
});

describe("detectsNonDeterminism", () => {
  const cases: [string, string | null][] = [
    ["function f() { return Math.random(); }", "Math.random"],
    ["const t = Date.now();", "Date.now"],
    ["performance.now()", "performance.now"],
    ["const d = new Date();", "new Date()"],
    ["crypto.randomUUID()", "crypto random"],
    ["const x = 5; return x + 1;", null],
    ["function f(a) { return a * 2; }", null],
    ["// Math.random is not called, just mentioned in a string\nconst s = 'Math.random';", "Math.random"],
  ];
  for (const [source, expected] of cases) {
    it(`${JSON.stringify(source.slice(0, 40))} → ${expected}`, () => {
      expect(pct.detectsNonDeterminism(source)).toBe(expected);
    });
  }
});
