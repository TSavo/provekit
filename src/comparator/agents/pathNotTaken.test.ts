import { describe, it, expect } from "vitest";
import { pathNotTakenAgent } from "./pathNotTaken.js";

describe("pathNotTakenAgent", () => {
  it("reports path_not_taken when the runtime did not visit the signal line", () => {
    const gap = pathNotTakenAgent({
      signalLine: 10,
      visitedLines: new Set([1, 2, 3, 4, 5]),
      smtConstant: "unreachable",
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("path_not_taken");
    expect(gap!.explanation).toMatch(/line 10/);
  });

  it("returns null when the runtime did visit the signal line", () => {
    const gap = pathNotTakenAgent({
      signalLine: 3,
      visitedLines: new Set([1, 2, 3, 4]),
      smtConstant: "ok",
    });
    expect(gap).toBeNull();
  });

  it("returns null when visitedLines is empty (no trace captured)", () => {
    const gap = pathNotTakenAgent({
      signalLine: 3,
      visitedLines: new Set(),
      smtConstant: "ok",
    });
    expect(gap).toBeNull();
  });
});
