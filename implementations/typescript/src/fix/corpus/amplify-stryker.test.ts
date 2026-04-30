/**
 * Smoke tests for amplify-stryker. We're not asserting principle behaviour
 * here — just that the operator catalog produces deterministic, syntactically
 * meaningful mutations on the curated bases, and that preservation tagging
 * filters correctly.
 */

import { describe, it, expect } from "vitest";
import { amplifyScenario } from "./amplify-stryker.js";
import { scenario as ternary002 } from "./scenarios/ternary-branch-collapse/ternary-002.js";
import { scenario as advUnsat } from "./scenarios/adversarial/adv-unsatisfiable-invariant.js";
import { scenario as advOOS } from "./scenarios/adversarial/adv-out-of-scope.js";
import { scenario as dbz001 } from "./scenarios/division-by-zero/dbz-001.js";

describe("amplify-stryker", () => {
  it("produces preserves_bug mutations for ternary-002 (has `>=` comparator)", () => {
    const amplified = amplifyScenario(ternary002, { maxMutations: 20 });
    expect(amplified.length).toBeGreaterThan(0);
    for (const m of amplified) {
      expect(m.baseScenarioId).toBe("ternary-002");
      expect(m.preservation).toBe("preserves_bug");
      expect(m.id.startsWith("ternary-002+m")).toBe(true);
      expect(m.files["src/status.ts"]).not.toBe(ternary002.files["src/status.ts"]);
    }
  });

  it("never amplifies an out_of_scope base", () => {
    const amplified = amplifyScenario(advOOS, { maxMutations: 20 });
    expect(amplified.length).toBe(0);
  });

  it("does not return removes_bug variants by default", () => {
    // dbz-001 has only `/` which classifies as removes_bug; everything else
    // requires another operator that doesn't appear in its body.
    const amplified = amplifyScenario(dbz001, { maxMutations: 50 });
    for (const m of amplified) {
      expect(m.preservation).not.toBe("removes_bug");
    }
  });

  it("respects maxMutations cap", () => {
    const amplified = amplifyScenario(advUnsat, { maxMutations: 1 });
    expect(amplified.length).toBeLessThanOrEqual(1);
  });

  it("includes operator and arrow in mutationKind", () => {
    const amplified = amplifyScenario(ternary002, { maxMutations: 5 });
    expect(amplified.length).toBeGreaterThan(0);
    for (const m of amplified) {
      expect(m.mutationKind).toMatch(/^[a-z]+:.+=>.+$/);
    }
  });
});
