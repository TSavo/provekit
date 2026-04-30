/**
 * IR → CVC5 SMT-LIB translator tests.
 *
 * The deep coverage of formula emission lives in src/ir/smt/smt.test.ts.
 * Here we assert the CVC5 wrapper's distinct shape: produce-models
 * option is present, set-logic is present, the script ends in
 * (check-sat), and axioms thread through.
 */

import { describe, it, expect } from "vitest";
import { emitCvc5SmtLib, emitCvc5SmtLibProblem } from "./index.js";
import type { IrFormula } from "../formulas.js";

const trueFormula: IrFormula = {
  kind: "atomic",
  name: "=",
  args: [
    { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } },
    { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } },
  ],
};

const positiveExists: IrFormula = {
  kind: "exists", name: "_x0", sort: { kind: "primitive", name: "Int" }, body: {
      kind: "atomic",
      name: ">",
      args: [
        { kind: "var", name: "_x0" },
        { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
      ],
    },
};

describe("emitCvc5SmtLib", () => {
  it("renders a single formula without preamble", () => {
    const out = emitCvc5SmtLib(trueFormula);
    expect(out).toBe("(= 1 1)");
  });

  it("renders a quantified formula identically to the Z3 path body", () => {
    const out = emitCvc5SmtLib(positiveExists);
    expect(out).toContain("exists");
    expect(out).toContain("(> _x0 0)");
  });
});

describe("emitCvc5SmtLibProblem", () => {
  it("emits set-option :produce-models BEFORE set-logic", () => {
    const out = emitCvc5SmtLibProblem({ axioms: [], assertion: trueFormula });
    const optIdx = out.indexOf("(set-option :produce-models true)");
    const logicIdx = out.indexOf("(set-logic ALL)");
    expect(optIdx).toBeGreaterThanOrEqual(0);
    expect(logicIdx).toBeGreaterThanOrEqual(0);
    expect(optIdx).toBeLessThan(logicIdx);
  });

  it("ends in (check-sat) and asserts the NEGATION of the assertion", () => {
    const out = emitCvc5SmtLibProblem({ axioms: [], assertion: trueFormula });
    expect(out).toContain("(check-sat)");
    expect(out).toContain("(assert (not (= 1 1)))");
  });

  it("honors a non-default logic", () => {
    const out = emitCvc5SmtLibProblem({
      axioms: [],
      assertion: trueFormula,
      logic: "QF_LIA",
    });
    expect(out).toContain("(set-logic QF_LIA)");
  });

  it("threads axioms into the script before the negated assertion", () => {
    const axiom: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
        { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
      ],
    };
    const out = emitCvc5SmtLibProblem({
      axioms: [axiom],
      assertion: positiveExists,
    });
    expect(out).toContain("axioms");
    const assertCount = (out.match(/\(assert /g) ?? []).length;
    expect(assertCount).toBeGreaterThanOrEqual(2);
    const axiomIdx = out.indexOf("(assert (= 0 0))");
    const negatedIdx = out.indexOf("(assert (not");
    expect(axiomIdx).toBeGreaterThanOrEqual(0);
    expect(negatedIdx).toBeGreaterThan(axiomIdx);
  });

  it("tags the script with a cvc5-flavored header so dialect is visible", () => {
    const out = emitCvc5SmtLibProblem({ axioms: [], assertion: trueFormula });
    expect(out).toContain("cvc5-flavored");
  });
});
