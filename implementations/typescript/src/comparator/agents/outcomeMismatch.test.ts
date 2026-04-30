import { describe, it, expect } from "vitest";
import { outcomeMismatchAgent } from "./outcomeMismatch.js";

describe("outcomeMismatchAgent", () => {
  it("reports outcome_mismatch when SMT modeled a return but runtime threw", () => {
    const gap = outcomeMismatchAgent({
      smtOutcome: { kind: "returned" },
      runtimeOutcome: { kind: "threw", error: "TypeError: cannot read properties" },
      smtConstant: "result",
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("outcome_mismatch");
    expect(gap!.explanation).toMatch(/return.*threw/i);
  });

  it("reports when SMT modeled a throw but runtime returned", () => {
    const gap = outcomeMismatchAgent({
      smtOutcome: { kind: "threw" },
      runtimeOutcome: { kind: "returned" },
      smtConstant: "result",
    });
    expect(gap).not.toBeNull();
    expect(gap!.explanation).toMatch(/throw.*returned/i);
  });

  it("returns null when both outcomes agree", () => {
    const gap = outcomeMismatchAgent({
      smtOutcome: { kind: "returned" },
      runtimeOutcome: { kind: "returned" },
      smtConstant: "result",
    });
    expect(gap).toBeNull();
  });
});
