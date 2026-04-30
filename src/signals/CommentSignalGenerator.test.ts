import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { CommentSignalGenerator } from "./CommentSignalGenerator";
import { parseFile } from "../parser";

describe("CommentSignalGenerator", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("emits a signal for a comment inside a function", () => {
    const src = `
function divide(a: number, b: number): number {
  // ensure divisor is non-zero before divide
  return a / b;
}
`.trim();
    const tree = parseFile(src);
    const sigs = new CommentSignalGenerator().findSignals("d.ts", src, tree);
    expect(sigs.length).toBe(1);
    expect(sigs[0].type).toBe("comment");
    expect(sigs[0].text).toBe("ensure divisor is non-zero before divide");
    expect(sigs[0].functionName).toBe("divide");
    expect(sigs[0].parameters).toEqual([
      { name: "a", type: "number" },
      { name: "b", type: "number" },
    ]);
    expect(sigs[0].returnType).toBe("number");
  });

  it("skips tooling directives like eslint-disable and ts-ignore", () => {
    const src = `
function noop() {
  // eslint-disable-next-line no-unused-vars
  // @ts-ignore: legacy
  const x = 1;
}
`.trim();
    const tree = parseFile(src);
    const sigs = new CommentSignalGenerator().findSignals("n.ts", src, tree);
    expect(sigs).toEqual([]);
  });

  it("skips comments outside any function (module scope)", () => {
    const src = `
// module-level comment, no enclosing function
const X = 1;
`.trim();
    const tree = parseFile(src);
    const sigs = new CommentSignalGenerator().findSignals("m.ts", src, tree);
    expect(sigs).toEqual([]);
  });

  it("strips leading // and block comment markers", () => {
    const src = `
function f(x: number): number {
  /* canonical block comment */
  return x;
}
`.trim();
    const tree = parseFile(src);
    const sigs = new CommentSignalGenerator().findSignals("b.ts", src, tree);
    expect(sigs).toHaveLength(1);
    expect(sigs[0].text).toBe("canonical block comment");
  });

  it("skips comments with cleaned text under 3 chars", () => {
    const src = `
function f(): void {
  // ab
}
`.trim();
    const tree = parseFile(src);
    const sigs = new CommentSignalGenerator().findSignals("s.ts", src, tree);
    expect(sigs).toEqual([]);
  });
});
