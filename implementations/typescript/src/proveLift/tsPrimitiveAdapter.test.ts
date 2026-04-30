/**
 * Tests for the v0 prove-lift adapter.
 *
 * Detect-stage tests are real and must pass. Propose / Filter / Review
 * tests verify the stub behavior so the wiring contract stays stable
 * while run-2 fills in the real implementations.
 *
 * The CID-equivalence test (lift produces propertyHash 8c38f05152707736
 * matching the hand-authored fixture) is intentionally a `it.fails` /
 * `it.todo` placeholder; landing it green is the run-2 acceptance gate.
 */

import { describe, it, expect } from "vitest";
import { resolve } from "node:path";

import { detect } from "./detect.js";
import { propose } from "./propose.js";
import { filter } from "./filter.js";
import { review } from "./review.js";
import { tsPrimitiveAdapter } from "./tsPrimitiveAdapter.js";
import { liftFile } from "./index.js";
import { LiftError } from "./errors.js";

const FIX = (name: string) => resolve(__dirname, "__fixtures__", name);

describe("proveLift / Detect (real)", () => {
  it("extracts parseInt(s: string): number as String -> Int", () => {
    const r = detect(FIX("parseInt.ts"));
    expect(r.shape.name).toBe("parseInt");
    expect(r.shape.params).toEqual([
      { name: "s", sort: { kind: "primitive", name: "String" } },
    ]);
    expect(r.shape.returnSort).toEqual({ kind: "primitive", name: "Int" });
    expect(r.diagnostics).toEqual([]);
  });

  it("captures the function source for downstream LLM context", () => {
    const r = detect(FIX("parseInt.ts"));
    expect(r.shape.functionSource).toMatch(/export function parseInt\b/);
    expect(r.shape.sourceText).toContain("function parseInt");
  });

  it("refuses multi-export files with `multiple-exports`", () => {
    expect(() => detect(FIX("multiExport.ts"))).toThrow(LiftError);
    try {
      detect(FIX("multiExport.ts"));
    } catch (err) {
      const e = err as LiftError;
      expect(e.diagnostic.code).toBe("multiple-exports");
      expect(e.diagnostic.message).toContain("alpha");
      expect(e.diagnostic.message).toContain("beta");
    }
  });

  it("refuses non-primitive parameter types with `non-primitive-surface`", () => {
    expect(() => detect(FIX("nonPrimitive.ts"))).toThrow(LiftError);
    try {
      detect(FIX("nonPrimitive.ts"));
    } catch (err) {
      const e = err as LiftError;
      expect(e.diagnostic.code).toBe("non-primitive-surface");
      expect(e.diagnostic.detail ?? "").toMatch(/number\[\]|Array<number>/);
    }
  });

  it("refuses files with no exports with `no-exports`", () => {
    expect(() => detect(FIX("noExports.ts"))).toThrow(LiftError);
    try {
      detect(FIX("noExports.ts"));
    } catch (err) {
      const e = err as LiftError;
      expect(e.diagnostic.code).toBe("no-exports");
    }
  });
});

describe("proveLift / Propose (stub)", () => {
  it("substitutes function context into the intake prompt", async () => {
    const { shape } = detect(FIX("parseInt.ts"));
    const r = await propose(shape);
    expect(r.prompt).toContain("Function: `parseInt`");
    expect(r.prompt).toContain("forall s: String");
    expect(r.prompt).toContain("export function parseInt");
  });

  it("returns hardcoded parseInt candidates when no LLM is wired", async () => {
    const { shape } = detect(FIX("parseInt.ts"));
    const r = await propose(shape);
    expect(r.candidates.length).toBeGreaterThanOrEqual(3);
    expect(r.candidates[0]!.body).toContain("parseInt(String(n))");
  });
});

describe("proveLift / Filter (stub)", () => {
  it("passes all candidates through (stub behavior)", async () => {
    const { shape } = detect(FIX("parseInt.ts"));
    const p = await propose(shape);
    const f = await filter(shape, p.candidates);
    expect(f.survivors).toHaveLength(p.candidates.length);
    expect(f.notes.every((n) => !n.dropped)).toBe(true);
  });
});

describe("proveLift / Review (stub)", () => {
  it("auto-accepts the first candidate", async () => {
    const { shape } = detect(FIX("parseInt.ts"));
    const p = await propose(shape);
    const r = await review(shape, p.candidates);
    expect(r.accepted).toHaveLength(1);
    expect(r.accepted[0]).toEqual(p.candidates[0]);
  });
});

describe("proveLift / adapter detect-score", () => {
  it("scores parseInt.ts at 1.0", () => {
    expect(tsPrimitiveAdapter.detectScore(FIX("parseInt.ts"))).toBe(1);
  });

  it("scores .invariant.ts at 0", () => {
    // .invariant.ts is the existing IR-lifter's territory, not lift's.
    expect(tsPrimitiveAdapter.detectScore("/tmp/foo.invariant.ts")).toBe(0);
  });

  it("scores .test.ts at 0", () => {
    expect(tsPrimitiveAdapter.detectScore("/tmp/foo.test.ts")).toBe(0);
  });
});

describe("proveLift / liftFile end-to-end (Mint stubbed -> throws)", () => {
  // Run-2 acceptance gate: this test should produce a real .proof and
  // the propertyHash should match the hand-authored fixture.
  it.todo(
    "parseInt.ts -> .proof with propertyHash 8c38f05152707736 (CID-equivalent to the hand-authored fixture)",
  );

  it("currently throws at Mint stage with a loud not-yet-implemented error", async () => {
    await expect(liftFile(FIX("parseInt.ts"))).rejects.toThrow(
      /mint: not yet implemented/,
    );
  });
});

describe("proveLift / cross-language round-trip (run-3 acceptance)", () => {
  it.todo("minted .proof verifies cleanly under the TS verifier");
  it.todo("minted .proof verifies cleanly under the Go verifier");
  it.todo("minted .proof verifies cleanly under the C++ verifier");
});
