import { describe, it, expect } from "vitest";
import { splitCapturedEdits } from "./doTheWork.js";

describe("splitCapturedEdits", () => {
  it("routes .test.ts files to testEdits, others to sourceEdits", () => {
    const patch = {
      fileEdits: [
        { file: "src/calc.ts", newContent: "function divide(a,b) { ... }" },
        { file: "src/calc.regression.test.ts", newContent: "it('throws on zero', ...)" },
      ],
      description: "captured",
    };
    const { sourceEdits, testEdits } = splitCapturedEdits(patch);
    expect(sourceEdits.map((e) => e.file)).toEqual(["src/calc.ts"]);
    expect(testEdits.map((e) => e.file)).toEqual(["src/calc.regression.test.ts"]);
  });

  it("recognizes .spec.ts and __tests__/ paths", () => {
    const patch = {
      fileEdits: [
        { file: "src/foo.ts", newContent: "..." },
        { file: "src/foo.spec.ts", newContent: "..." },
        { file: "src/__tests__/bar.ts", newContent: "..." },
        { file: "tests/integration/baz.ts", newContent: "..." },
      ],
      description: "captured",
    };
    const { sourceEdits, testEdits } = splitCapturedEdits(patch);
    expect(sourceEdits.map((e) => e.file)).toEqual(["src/foo.ts"]);
    expect(testEdits.map((e) => e.file).sort()).toEqual([
      "src/__tests__/bar.ts",
      "src/foo.spec.ts",
      "tests/integration/baz.ts",
    ]);
  });

  it("returns empty arrays for an empty patch", () => {
    const { sourceEdits, testEdits } = splitCapturedEdits({
      fileEdits: [],
      description: "",
    });
    expect(sourceEdits).toEqual([]);
    expect(testEdits).toEqual([]);
  });
});
