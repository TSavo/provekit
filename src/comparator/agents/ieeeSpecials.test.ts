import { describe, it, expect } from "vitest";
import { ieeeSpecialsAgent } from "./ieeeSpecials.js";

describe("ieeeSpecialsAgent", () => {
  it("reports NaN when SMT modeled a finite Real but runtime observed NaN", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Real", sourceLine: 3, sourceExpr: "a/b" },
      witness: { sort: "Real", value: 0 },
      runtimeValue: { kind: "nan" },
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("ieee_specials");
    expect(gap!.explanation).toMatch(/NaN/);
  });

  it("reports Infinity when SMT said div_by_zero and runtime observed Infinity", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Real", sourceLine: 3, sourceExpr: "a/b" },
      witness: { sort: "Real", value: "div_by_zero" },
      runtimeValue: { kind: "infinity" },
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("ieee_specials");
    expect(gap!.explanation).toMatch(/Infinity/);
  });

  it("returns null when SMT value matches runtime value numerically", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Real", sourceLine: 3, sourceExpr: "a/b" },
      witness: { sort: "Real", value: 2.5 },
      runtimeValue: { kind: "number", numberValue: 2.5 },
    });
    expect(gap).toBeNull();
  });

  it("skips non-Real sorts", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Bool", sourceLine: 3, sourceExpr: "a" },
      witness: { sort: "Bool", value: true },
      runtimeValue: { kind: "bool", boolValue: true },
    });
    expect(gap).toBeNull();
  });
});
