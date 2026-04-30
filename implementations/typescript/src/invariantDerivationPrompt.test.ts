import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { join } from "path";

describe("invariant_derivation prompt — binding metadata section", () => {
  const promptPath = join(__dirname, "..", "prompts", "invariant_derivation.md");
  const promptText = readFileSync(promptPath, "utf-8");

  it("teaches the LLM to emit smt_constant, source_line, source_expr, and sort for every declared constant", () => {
    expect(promptText).toMatch(/smt_constant/);
    expect(promptText).toMatch(/source_line/);
    expect(promptText).toMatch(/source_expr/);
    expect(promptText).toMatch(/\bsort\b/);
  });

  it("shows a worked example pairing an smt2 block with a bindings block", () => {
    expect(promptText).toMatch(/"smt_constant"\s*:\s*"\w+"/);
    expect(promptText).toMatch(/```smt2[\s\S]*?```\s*\n[\s\S]*?```bindings/);
  });

  it("documents the abstract sentinel for constants with no source correspondent", () => {
    expect(promptText).toMatch(/"source_line"\s*:\s*0/);
    expect(promptText).toMatch(/<abstract>/);
  });
});
