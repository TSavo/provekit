import { describe, it, expect } from "vitest";
import { computeSignalHash, Signal } from "./Signal";

function makeSignal(overrides: Partial<Signal> = {}): Signal {
  return {
    file: "src/foo.ts",
    line: 10,
    column: 4,
    type: "ast:branch",
    text: "if/else branch",
    functionName: "foo",
    functionSource: "function foo(x: number) { if (x > 0) {} else {} }",
    functionStartLine: 1,
    functionEndLine: 5,
    parameters: [{ name: "x", type: "number" }],
    returnType: "void",
    pathConditions: ["x > 0"],
    localTypes: { y: "string" },
    callees: ["console.log"],
    calledBy: [],
    ...overrides,
  };
}

describe("computeSignalHash", () => {
  it("returns a stable 64-hex-char SHA-256", () => {
    const h = computeSignalHash(makeSignal());
    expect(h).toMatch(/^[0-9a-f]{64}$/);
  });

  it("is deterministic across calls with the same input", () => {
    const a = computeSignalHash(makeSignal());
    const b = computeSignalHash(makeSignal());
    expect(a).toBe(b);
  });

  it("changes when functionSource differs", () => {
    const a = computeSignalHash(makeSignal({ functionSource: "function foo() {}" }));
    const b = computeSignalHash(makeSignal({ functionSource: "function foo(x) { return x; }" }));
    expect(a).not.toBe(b);
  });

  it("changes when path conditions differ", () => {
    const a = computeSignalHash(makeSignal({ pathConditions: [] }));
    const b = computeSignalHash(makeSignal({ pathConditions: ["x > 0"] }));
    expect(a).not.toBe(b);
  });

  it("ignores callees ordering (sorted before hashing)", () => {
    const a = computeSignalHash(makeSignal({ callees: ["a", "b", "c"] }));
    const b = computeSignalHash(makeSignal({ callees: ["c", "b", "a"] }));
    expect(a).toBe(b);
  });

  it("does not depend on calledBy", () => {
    const a = computeSignalHash(makeSignal({ calledBy: [] }));
    const b = computeSignalHash(makeSignal({ calledBy: ["caller1", "caller2"] }));
    expect(a).toBe(b);
  });
});
