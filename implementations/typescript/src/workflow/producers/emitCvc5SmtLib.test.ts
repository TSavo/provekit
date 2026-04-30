/**
 * emit-cvc5-smt-lib Stage tests. Wraps emitCvc5SmtLibProblem; the heavy
 * coverage lives in src/ir/cvc5/cvc5.test.ts. Here we assert the Stage
 * shape, output shape, and that it threads through the CVC5-flavored
 * preamble (produce-models option + dialect header).
 */

import { describe, it, expect } from "vitest";
import {
  makeEmitCvc5SmtLibStage,
  EMIT_CVC5_SMT_LIB_CAPABILITY,
} from "./emitCvc5SmtLib.js";
import type { IrFormula } from "../../ir/formulas.js";

const trueFormula: IrFormula = {
  kind: "atomic",
  predicate: "=",
  args: [
    { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } },
    { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } },
  ],
};

describe("emit-cvc5-smt-lib stage", () => {
  it("has the expected capability constant", () => {
    expect(EMIT_CVC5_SMT_LIB_CAPABILITY).toBe("emit-cvc5-smt-lib");
  });

  it("emits a CVC5-flavored script with produce-models, set-logic, check-sat", async () => {
    const stage = makeEmitCvc5SmtLibStage();
    const out = await stage.run({ formula: trueFormula });
    expect(out.smtLib).toContain("(set-option :produce-models true)");
    expect(out.smtLib).toContain("(set-logic ALL)");
    expect(out.smtLib).toContain("(check-sat)");
    expect(out.smtLib).toContain("(assert (not");
    expect(out.smtLib).toContain("cvc5-flavored");
    expect(out.logic).toBe("ALL");
  });

  it("honors a non-default logic", async () => {
    const stage = makeEmitCvc5SmtLibStage();
    const out = await stage.run({ formula: trueFormula, logic: "QF_LIA" });
    expect(out.smtLib).toContain("(set-logic QF_LIA)");
    expect(out.logic).toBe("QF_LIA");
  });

  it("serializeInput captures formula + axioms + logic for stable hashing", () => {
    const stage = makeEmitCvc5SmtLibStage();
    const a = stage.serializeInput({ formula: trueFormula });
    const b = stage.serializeInput({
      formula: trueFormula,
      axioms: [],
      logic: "ALL",
    });
    expect(a).toEqual(b);
  });

  it("output round-trips through serialize/deserialize", () => {
    const stage = makeEmitCvc5SmtLibStage();
    const sample = { smtLib: "(check-sat)\n", logic: "ALL" };
    const witness = stage.serializeOutput(sample);
    expect(stage.deserializeOutput(witness)).toEqual(sample);
  });

  it("uses a distinct producedBy from the Z3-flavored emitter", () => {
    const stage = makeEmitCvc5SmtLibStage();
    expect(stage.producedBy).toContain("cvc5");
  });
});
