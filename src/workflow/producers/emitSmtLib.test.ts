/**
 * emit-smt-lib Stage tests. Wraps emitSmtLibProblem; the heavy
 * coverage of SMT translation lives in src/ir/smt/smt.test.ts.
 * Here we assert the Stage shape, output shape, and pass-through.
 */

import { describe, it, expect } from "vitest";
import {
  makeEmitSmtLibStage,
  EMIT_SMT_LIB_CAPABILITY,
} from "./emitSmtLib.js";
import type { IrFormula } from "../../ir/formulas.js";

const trueFormula: IrFormula = {
  kind: "atomic",
  predicate: "=",
  args: [
    { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } },
    { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } },
  ],
};

const positiveExists: IrFormula = {
  kind: "exists",
  sort: { kind: "primitive", name: "Int" },
  predicate: {
    kind: "lambda",
    varName: "_x0",
    sort: { kind: "primitive", name: "Int" },
    body: {
      kind: "atomic",
      predicate: ">",
      args: [
        { kind: "var", name: "_x0", sort: { kind: "primitive", name: "Int" } },
        { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
      ],
    },
  },
};

describe("emit-smt-lib stage", () => {
  it("has the expected capability constant", () => {
    expect(EMIT_SMT_LIB_CAPABILITY).toBe("emit-smt-lib");
  });

  it("emits a full SMT-LIB script with set-logic, assert, check-sat", async () => {
    const stage = makeEmitSmtLibStage();
    const out = await stage.run({ formula: trueFormula });
    expect(out.smtLib).toContain("(set-logic ALL)");
    expect(out.smtLib).toContain("(check-sat)");
    expect(out.smtLib).toContain("(assert (not");
    expect(out.logic).toBe("ALL");
  });

  it("honors a non-default logic", async () => {
    const stage = makeEmitSmtLibStage();
    const out = await stage.run({ formula: trueFormula, logic: "QF_LIA" });
    expect(out.smtLib).toContain("(set-logic QF_LIA)");
    expect(out.logic).toBe("QF_LIA");
  });

  it("threads axioms into the script", async () => {
    const axiom: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [
        { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
        { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
      ],
    };
    const stage = makeEmitSmtLibStage();
    const out = await stage.run({
      formula: positiveExists,
      axioms: [axiom],
    });
    expect(out.smtLib).toContain("axioms");
    // The axiom appears as its own (assert ...) line before the negated
    // assertion.
    const assertCount = (out.smtLib.match(/\(assert /g) ?? []).length;
    expect(assertCount).toBeGreaterThanOrEqual(2);
  });

  it("serializeInput captures formula + axioms + logic for stable hashing", () => {
    const stage = makeEmitSmtLibStage();
    const a = stage.serializeInput({ formula: trueFormula });
    const b = stage.serializeInput({
      formula: trueFormula,
      axioms: [],
      logic: "ALL",
    });
    expect(a).toEqual(b);
  });

  it("output round-trips through serialize/deserialize", () => {
    const stage = makeEmitSmtLibStage();
    const sample = { smtLib: "(check-sat)\n", logic: "ALL" };
    const witness = stage.serializeOutput(sample);
    expect(stage.deserializeOutput(witness)).toEqual(sample);
  });
});
