/**
 * Property-based tests for the fix loop invariants.
 *
 * Uses fast-check to generate random-but-valid scenarios from the registered
 * corpus and assert structural invariants that must hold regardless of input.
 *
 * If a property fails, fast-check shrinks the input to a minimal counterexample.
 *
 * Keep numRuns low (3-5) — each run spins up a full scratch project + fix loop.
 * These tests are for invariant discovery, not load testing.
 */

import { describe, it, expect } from "vitest";
import * as fc from "fast-check";

import { runScenarioIsolated } from "./corpus/runner.js";
import type { SweepResult } from "./corpus/runner.js";
import { ALL_SCENARIOS } from "./corpus/index.js";

// ---------------------------------------------------------------------------
// Arbitrary: pick a random scenario from the corpus
// ---------------------------------------------------------------------------

const arbitraryScenario = fc.constantFrom(...ALL_SCENARIOS);

// ---------------------------------------------------------------------------
// Helper: sweep ID unique to this test run
// ---------------------------------------------------------------------------

const SWEEP_ID = `prop-${Date.now()}`;

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

describe(
  "loop invariant properties",
  () => {
    /**
     * Property 1: If the outcome is "applied", the audit trail must contain
     * a "complete" entry for D1. A bundle that was applied required D1 to finish.
     */
    it(
      "property: applied outcome implies D1 complete in audit trail",
      { timeout: 180_000 },
      async () => {
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            if (result.actual.outcome === "applied") {
              const d1Complete = result.actual.auditTrail.some(
                (e) => e.stage === "D1" && e.kind === "complete",
              );
              expect(d1Complete).toBe(true);
            }
          }),
          { numRuns: 3 },
        );
      },
    );

    /**
     * Property 2: The stagesCompleted list must be a subset of the audit trail
     * entries with kind="complete". No stage can appear in stagesCompleted
     * unless the audit trail has a corresponding "complete" entry.
     */
    it(
      "property: stagesCompleted is consistent with audit trail",
      { timeout: 180_000 },
      async () => {
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            const auditCompleted = new Set(
              result.actual.auditTrail
                .filter((e) => e.kind === "complete")
                .map((e) => e.stage),
            );
            for (const stage of result.actual.stagesCompleted) {
              expect(auditCompleted.has(stage)).toBe(true);
            }
          }),
          { numRuns: 3 },
        );
      },
    );

    /**
     * Property 3: The outcome must be one of the four valid states.
     * The runner must never produce an undefined or garbage outcome.
     */
    it(
      "property: outcome is always a valid enum member",
      { timeout: 180_000 },
      async () => {
        const validOutcomes = new Set(["applied", "rejected", "out_of_scope", "errored"]);
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            expect(validOutcomes.has(result.actual.outcome)).toBe(true);
          }),
          { numRuns: 3 },
        );
      },
    );

    /**
     * Property 4: The classification must be one of the five valid values.
     */
    it(
      "property: classification is always a valid enum member",
      { timeout: 180_000 },
      async () => {
        const validClassifications = new Set([
          "match",
          "expected_failure",
          "integration_gap",
          "principle_rejection",
          "unknown",
        ]);
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            expect(validClassifications.has(result.classification)).toBe(true);
          }),
          { numRuns: 3 },
        );
      },
    );

    /**
     * Property 5: If failedStage is set, failureReason must also be set.
     * A stage failure without a reason is a runner bug.
     */
    it(
      "property: failedStage set implies failureReason is set",
      { timeout: 180_000 },
      async () => {
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            if (result.actual.failedStage !== undefined) {
              expect(result.actual.failureReason).toBeDefined();
              expect(typeof result.actual.failureReason).toBe("string");
            }
          }),
          { numRuns: 3 },
        );
      },
    );

    /**
     * Property 6: orchestrator caught error implies bundle is null.
     * If the orchestrator recorded an error, the fix loop must have returned null bundle.
     * This asserts the orchestrator's error-handling contract (from orchestrator.ts).
     */
    it(
      "property: orchestrator error in audit implies outcome is not applied",
      { timeout: 180_000 },
      async () => {
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            const hasOrchError = result.actual.auditTrail.some(
              (e) => e.stage === "orchestrator" && e.kind === "error",
            );
            if (hasOrchError) {
              expect(result.actual.outcome).not.toBe("applied");
            }
          }),
          { numRuns: 3 },
        );
      },
    );

    /**
     * Property 7: audit trail entries are always chronologically ordered.
     * No entry should have a timestamp earlier than its predecessor.
     */
    it(
      "property: audit trail timestamps are non-decreasing",
      { timeout: 180_000 },
      async () => {
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            const trail = result.actual.auditTrail;
            for (let i = 1; i < trail.length; i++) {
              expect(trail[i].timestamp).toBeGreaterThanOrEqual(trail[i - 1].timestamp);
            }
          }),
          { numRuns: 3 },
        );
      },
    );

    /**
     * Property 8: out_of_scope outcome never has D1 complete.
     * If the bug is out of scope, we should never have assembled a bundle.
     */
    it(
      "property: out_of_scope outcome never has D1 complete",
      { timeout: 180_000 },
      async () => {
        await fc.assert(
          fc.asyncProperty(arbitraryScenario, async (scenario) => {
            const result: SweepResult = await runScenarioIsolated(scenario, SWEEP_ID);
            if (result.actual.outcome === "out_of_scope") {
              const d1Complete = result.actual.auditTrail.some(
                (e) => e.stage === "D1" && e.kind === "complete",
              );
              expect(d1Complete).toBe(false);
            }
          }),
          { numRuns: 3 },
        );
      },
    );
  },
);
