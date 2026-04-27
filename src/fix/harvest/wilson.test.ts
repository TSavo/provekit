/**
 * Tests for Wilson score interval (the precision-CI helper for #115 step 5).
 * Reference values from R's `prop.test(k, n, conf.level=0.95, correct=FALSE)`
 * and Wikipedia's worked examples — both compute the Wilson score interval.
 */
import { describe, it, expect } from "vitest";
import { wilson, formatWilson } from "./wilson.js";

describe("wilson", () => {
  it("perfect successes (k=n): upper=1, lower<1, no overflow above 1", () => {
    const w = wilson(10, 10);
    expect(w.pointEstimate).toBe(1);
    expect(w.upper).toBeCloseTo(1, 6);
    expect(w.lower).toBeGreaterThan(0.5);
    expect(w.lower).toBeLessThan(1);
  });

  it("zero successes (k=0): lower=0, upper>0, no overflow below 0", () => {
    const w = wilson(0, 10);
    expect(w.pointEstimate).toBe(0);
    expect(w.lower).toBeCloseTo(0, 6);
    expect(w.upper).toBeGreaterThan(0);
    expect(w.upper).toBeLessThan(0.5);
  });

  it("k=11 / n=15: well-known sample, should produce p̂≈0.733", () => {
    // From the harvest sample: 11 agree out of 15 (a leave-one-out
    // illustration). The Wilson interval is roughly [0.481, 0.890]
    // at 95% (matches R's prop.test correct=FALSE).
    const w = wilson(11, 15);
    expect(w.pointEstimate).toBeCloseTo(0.7333, 3);
    expect(w.lower).toBeGreaterThan(0.45);
    expect(w.lower).toBeLessThan(0.51);
    expect(w.upper).toBeGreaterThan(0.88);
    expect(w.upper).toBeLessThan(0.90);
  });

  it("k=27 / n=30: the 90% gate threshold for #115", () => {
    // 27/30 = 0.9 — the literal precision the gate requires. Wilson CI
    // tells us how confident we should be in the point estimate.
    const w = wilson(27, 30);
    expect(w.pointEstimate).toBeCloseTo(0.9, 3);
    expect(w.lower).toBeGreaterThan(0.74);
    expect(w.upper).toBeLessThan(0.97);
  });

  it("symmetric around p=0.5 when k=n/2", () => {
    const w = wilson(5, 10);
    expect(w.pointEstimate).toBe(0.5);
    expect(Math.abs((w.upper - 0.5) - (0.5 - w.lower))).toBeLessThan(0.005);
  });

  it("smaller n → wider interval (fewer samples = less certainty)", () => {
    const small = wilson(1, 5);
    const large = wilson(10, 50);
    expect(small.upper - small.lower).toBeGreaterThan(large.upper - large.lower);
  });

  it("99% CI is wider than 95% CI for the same data", () => {
    const ci95 = wilson(11, 15, 0.95);
    const ci99 = wilson(11, 15, 0.99);
    expect(ci99.upper - ci99.lower).toBeGreaterThan(ci95.upper - ci95.lower);
  });

  it("rejects unsupported confidence levels", () => {
    expect(() => wilson(5, 10, 0.80 as any)).toThrow();
  });

  it("rejects invalid k/n", () => {
    expect(() => wilson(11, 10)).toThrow();
    expect(() => wilson(-1, 10)).toThrow();
  });

  it("n=0 returns NaN bounds without throwing", () => {
    const w = wilson(0, 0);
    expect(Number.isNaN(w.lower)).toBe(true);
    expect(Number.isNaN(w.upper)).toBe(true);
  });

  it("formatWilson includes the canonical pieces", () => {
    const s = formatWilson(wilson(27, 30));
    expect(s).toMatch(/k=27\/30/);
    expect(s).toMatch(/95%/);
    expect(s).toMatch(/0\.9/);
  });
});
