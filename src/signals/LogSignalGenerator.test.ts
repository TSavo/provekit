import { describe, it, expect } from "vitest";
import { LogSignalGenerator } from "./LogSignalGenerator";
import { parseFile } from "../parser";

describe("LogSignalGenerator", () => {
  it("emits a signal for console.log call inside a function", () => {
    const src = `
function f(x: number): number {
  console.log("entering f", x);
  return x + 1;
}
`.trim();
    const tree = parseFile(src);
    const sigs = new LogSignalGenerator().findSignals("f.ts", src, tree);
    expect(sigs.length).toBeGreaterThanOrEqual(1);
    expect(sigs[0].type).toBe("log");
    expect(sigs[0].functionName).toBe("f");
  });

  it("returns no signals for module-scope log calls", () => {
    const src = `console.log("at module scope")`;
    const tree = parseFile(src);
    const sigs = new LogSignalGenerator().findSignals("m.ts", src, tree);
    // Module-scope log calls cannot find an enclosing function and are dropped.
    expect(sigs).toEqual([]);
  });

  it("returns no signals when file has no log calls", () => {
    const src = "function f(): number { return 1; }";
    const tree = parseFile(src);
    const sigs = new LogSignalGenerator().findSignals("f.ts", src, tree);
    expect(sigs).toEqual([]);
  });
});
