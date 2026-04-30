/**
 * compare-verdicts Stage tests. Pure Stage; each branch mapped to the
 * shared property-verdict alphabet, agreement vs disagreement detected.
 */

import { describe, it, expect } from "vitest";
import {
  makeCompareVerdictsStage,
  COMPARE_VERDICTS_CAPABILITY,
} from "./compareVerdicts.js";

describe("compare-verdicts stage", () => {
  it("has the expected capability constant", () => {
    expect(COMPARE_VERDICTS_CAPABILITY).toBe("compare-verdicts");
  });

  it("agrees when both solvers return unsat (property holds)", async () => {
    const stage = makeCompareVerdictsStage();
    const out = await stage.run({ z3Verdict: "unsat", cvc5Verdict: "unsat" });
    expect(out.agree).toBe(true);
    expect(out.agreedVerdict).toBe("holds");
    expect(out.z3PropertyVerdict).toBe("holds");
    expect(out.cvc5PropertyVerdict).toBe("holds");
  });

  it("agrees when both solvers return sat (property violated)", async () => {
    const stage = makeCompareVerdictsStage();
    const out = await stage.run({ z3Verdict: "sat", cvc5Verdict: "sat" });
    expect(out.agree).toBe(true);
    expect(out.agreedVerdict).toBe("violated");
  });

  it("agrees when both solvers return undecidable (unknown / timeout)", async () => {
    const stage = makeCompareVerdictsStage();
    const out = await stage.run({ z3Verdict: "unknown", cvc5Verdict: "timeout" });
    expect(out.agree).toBe(true);
    expect(out.agreedVerdict).toBe("undecidable");
  });

  it("disagrees when one solver says holds and the other violated", async () => {
    const stage = makeCompareVerdictsStage();
    const out = await stage.run({ z3Verdict: "unsat", cvc5Verdict: "sat" });
    expect(out.agree).toBe(false);
    expect(out.agreedVerdict).toBeUndefined();
    expect(out.z3PropertyVerdict).toBe("holds");
    expect(out.cvc5PropertyVerdict).toBe("violated");
  });

  it("disagrees when z3 resolves but cvc5 returns unknown", async () => {
    const stage = makeCompareVerdictsStage();
    const out = await stage.run({ z3Verdict: "unsat", cvc5Verdict: "unknown" });
    expect(out.agree).toBe(false);
    expect(out.z3PropertyVerdict).toBe("holds");
    expect(out.cvc5PropertyVerdict).toBe("undecidable");
  });

  it("serializeInput captures both verdicts for stable hashing", () => {
    const stage = makeCompareVerdictsStage();
    const a = stage.serializeInput({ z3Verdict: "unsat", cvc5Verdict: "unsat" });
    const b = stage.serializeInput({ z3Verdict: "unsat", cvc5Verdict: "unsat" });
    expect(a).toEqual(b);
    const c = stage.serializeInput({ z3Verdict: "sat", cvc5Verdict: "unsat" });
    expect(a).not.toEqual(c);
  });

  it("output round-trips through serialize/deserialize", () => {
    const stage = makeCompareVerdictsStage();
    const sample = {
      agree: true,
      agreedVerdict: "holds" as const,
      z3Verdict: "unsat" as const,
      cvc5Verdict: "unsat" as const,
      z3PropertyVerdict: "holds" as const,
      cvc5PropertyVerdict: "holds" as const,
    };
    const witness = stage.serializeOutput(sample);
    expect(stage.deserializeOutput(witness)).toEqual(sample);
  });
});
