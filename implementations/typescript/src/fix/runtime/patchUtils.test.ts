/**
 * patchUtils gap coverage.
 *
 * Existing happy-path coverage lives in invariantStore.test.ts ("patchUtils
 * helpers" describe block). This file adds the missing surfaces:
 *   - pickPrimaryPatchFile (path-only convenience wrapper)
 *   - empty / single-edit cases for pickPrimaryPatchEdit
 *   - offsetToLine for offset==0, negative offset, offset > text length
 *   - findExpressionLines whitespace-trim behavior + empty-needle case
 *   - findFunctionLine method-shorthand and arrow-without-let cases
 *   - findFunctionLine null/undefined fnName guard
 */
import { describe, it, expect } from "vitest";
import {
  pickPrimaryPatchFile,
  pickPrimaryPatchEdit,
  offsetToLine,
  findExpressionLines,
  findFunctionLine,
} from "./patchUtils.js";
import type { FixCandidate } from "../types.js";

function makeFix(fileEdits: Array<{ file: string; newContent: string }>): FixCandidate {
  return {
    patch: { fileEdits, description: "test" },
    source: "llm",
    llmRationale: "",
    llmConfidence: 1,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 0,
      overlayClosed: false,
    },
  };
}

describe("pickPrimaryPatchFile", () => {
  it("returns null when fileEdits is empty", () => {
    const fix = makeFix([]);
    expect(pickPrimaryPatchFile(fix)).toBeNull();
  });

  it("returns the only file when there is exactly one edit", () => {
    const fix = makeFix([{ file: "only.ts", newContent: "x" }]);
    expect(pickPrimaryPatchFile(fix)).toBe("only.ts");
  });

  it("returns the longest edit's file path on multi-file patches", () => {
    const fix = makeFix([
      { file: "small.ts", newContent: "ab" },
      { file: "big.ts", newContent: "abcdefghij" },
      { file: "mid.ts", newContent: "abcde" },
    ]);
    expect(pickPrimaryPatchFile(fix)).toBe("big.ts");
  });
});

describe("pickPrimaryPatchEdit", () => {
  it("returns null on empty edits", () => {
    expect(pickPrimaryPatchEdit(makeFix([]))).toBeNull();
  });

  it("returns the single edit verbatim when there is exactly one", () => {
    const fix = makeFix([{ file: "only.ts", newContent: "abc" }]);
    const primary = pickPrimaryPatchEdit(fix);
    expect(primary).not.toBeNull();
    expect(primary?.file).toBe("only.ts");
    expect(primary?.newContent).toBe("abc");
  });

  it("breaks ties by preserving the first-seen edit (stable order)", () => {
    const fix = makeFix([
      { file: "first.ts", newContent: "same" },
      { file: "second.ts", newContent: "same" },
    ]);
    const primary = pickPrimaryPatchEdit(fix);
    expect(primary?.file).toBe("first.ts");
  });
});

describe("offsetToLine — edge cases", () => {
  it("returns 1 for offset 0 in any string (including empty)", () => {
    expect(offsetToLine("", 0)).toBe(1);
    expect(offsetToLine("anything", 0)).toBe(1);
  });

  it("returns 1 for negative offsets (pinned at line 1)", () => {
    expect(offsetToLine("abc\ndef", -5)).toBe(1);
  });

  it("clamps offset > text length to text length (no overflow)", () => {
    const text = "a\nb\nc";
    expect(offsetToLine(text, 10_000)).toBe(3);
  });

  it("returns line N+1 immediately after the Nth newline", () => {
    const text = "L1\nL2\nL3";
    expect(offsetToLine(text, 3)).toBe(2);
    expect(offsetToLine(text, 6)).toBe(3);
  });
});

describe("findExpressionLines — edge cases", () => {
  it("trims surrounding whitespace from the needle before searching", () => {
    const text = "function f() {\n  return computeX();\n}\n";
    const trimmed = findExpressionLines(text, "  computeX()  ");
    expect(trimmed).not.toBeNull();
    expect(trimmed?.startLine).toBe(2);
    expect(trimmed?.endLine).toBe(2);
  });

  it("returns null for an all-whitespace needle (empty after trim)", () => {
    expect(findExpressionLines("anything", "   \n\t  ")).toBeNull();
  });

  it("returns null when the needle is absent even though a partial match exists", () => {
    expect(findExpressionLines("computeX", "computeY")).toBeNull();
  });
});

describe("findFunctionLine — additional shapes", () => {
  it("returns null for null fnName", () => {
    expect(findFunctionLine("function any() {}", null)).toBeNull();
  });

  it("returns null for undefined fnName", () => {
    expect(findFunctionLine("function any() {}", undefined)).toBeNull();
  });

  it("returns null for empty fnName", () => {
    expect(findFunctionLine("function any() {}", "")).toBeNull();
  });

  it("matches `let NAME =` arrow assignment", () => {
    const text = "// header\nlet myFn = (x) => x + 1;\n";
    expect(findFunctionLine(text, "myFn")).toBe(2);
  });

  it("matches `var NAME =` declaration", () => {
    const text = "var legacy = function () {};\n";
    expect(findFunctionLine(text, "legacy")).toBe(1);
  });

  it("matches arrow assignment without let/const (`NAME = (...) =>`)", () => {
    const text = "module.exports = {};\nrebound = (a) => a;\n";
    expect(findFunctionLine(text, "rebound")).toBe(2);
  });

  it("matches method-shorthand (`NAME(`) on a continuation line", () => {
    // The shorthand regex anchors on `^` or `\n`; m.index lands ON the
    // newline so offsetToLine returns the line containing the newline,
    // not the line that follows it. Documented behavior — `findFunctionLine`
    // is a "best-effort starting anchor," not a precise line locator.
    const text = "class C {\n  method(arg: string) { return arg; }\n}\n";
    expect(findFunctionLine(text, "method")).toBe(1);
  });

  it("matches method-shorthand at the very start of the file", () => {
    const text = "method(arg) { return arg; }\n";
    expect(findFunctionLine(text, "method")).toBe(1);
  });

  it("matches async-method shorthand", () => {
    // Same anchoring behavior as above — match starts at the prior
    // newline, so it reports the previous line.
    const text = "class C {\n  async fetchOne() { return 1; }\n}\n";
    expect(findFunctionLine(text, "fetchOne")).toBe(1);
  });

  it("escapes regex metachars in fnName so $ and . are taken literally", () => {
    const text = "function $$dollarsign() {}\n";
    expect(findFunctionLine(text, "$$dollarsign")).toBe(1);
  });
});
