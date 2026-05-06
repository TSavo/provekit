import { describe, it, expect } from "vitest";
import { ForwardPropagator } from "./forward_propagator.js";

describe("ForwardPropagator", () => {
  const fp = new ForwardPropagator();
  fp.addToCatalog("checkPositive", 
    { constraints: ["x > 0"], isTop: false },
    { constraints: ["x <= 0"], isTop: false }
  );

  it("callsite satisfies pre - no diagnostic", () => {
    const currentPost = { constraints: ["x > 0"], isTop: false };
    const result = fp.checkCallsite("checkPositive", currentPost);
    expect(result).toBeNull();
  });

  it("callsite violates pre - diagnostic emitted", () => {
    const currentPost = { constraints: ["x <= 0"], isTop: false };
    const result = fp.checkCallsite("checkPositive", currentPost);
    expect(result).not.toBeNull();
    expect(result?.code).toBe("implication-failed");
  });

  it("callsite partial satisfaction - diagnostic emitted", () => {
    const currentPost = { constraints: ["x > 0", "y > 0"], isTop: false };
    const result = fp.checkCallsite("checkPositive", currentPost);
    expect(result).not.toBeNull();
    expect(result?.code).toBe("implication-failed");
  });

  it("top fallback - no diagnostic", () => {
    const currentPost = { constraints: [], isTop: true };
    const result = fp.checkCallsite("checkPositive", currentPost);
    expect(result).toBeNull();
  });

  it("unknown callee - no diagnostic", () => {
    const currentPost = { constraints: ["x > 0"], isTop: false };
    const result = fp.checkCallsite("unknown", currentPost);
    expect(result).toBeNull();
  });

  it("empty constraints - no diagnostic", () => {
    const currentPost = { constraints: [], isTop: false };
    const result = fp.checkCallsite("checkPositive", currentPost);
    expect(result).toBeNull();
  });
});