/**
 * Corpus runner tests: runs a minimal subset of the corpus (1-2 scenarios)
 * and asserts the SweepDashboard has the correct shape.
 *
 * Deliberately small: each scenario requires git init + SAST build + fix loop,
 * so this test is intentionally not exhaustive. Full corpus sweeps are via
 * `provekit fuzz` (scripts/fuzz.ts).
 */

import { describe, it, expect } from "vitest";

import { runSweep } from "./runner.js";
import { summarize } from "./dashboard.js";
import type { SweepDashboard } from "./dashboard.js";

// ---------------------------------------------------------------------------
// Mini corpus: one adversarial scenario that should produce expected_failure
// (locate fails on missing file) — fast and deterministic.
// ---------------------------------------------------------------------------

import { scenario as advMissingFile } from "./scenarios/adversarial/adv-missing-file.js";

describe(
  "corpus runner",
  () => {
    it(
      "runs a single-scenario sweep and returns a valid dashboard shape",
      { timeout: 60_000 },
      async () => {
        const results = await runSweep([advMissingFile]);
        expect(results).toHaveLength(1);

        const r = results[0];
        expect(r.scenarioId).toBe("adv-missing-file");

        // The dashboard shape is well-formed.
        const dashboard: SweepDashboard = summarize(results);
        expect(dashboard.totalScenarios).toBe(1);
        expect(typeof dashboard.integrationGapRate).toBe("number");
        expect(typeof dashboard.successRate).toBe("number");
        expect(dashboard.integrationGapRate).toBeGreaterThanOrEqual(0);
        expect(dashboard.integrationGapRate).toBeLessThanOrEqual(1);
        expect(dashboard.successRate).toBeGreaterThanOrEqual(0);
        expect(dashboard.successRate).toBeLessThanOrEqual(1);

        // Classification buckets all present.
        expect(typeof dashboard.classification.match).toBe("number");
        expect(typeof dashboard.classification.expected_failure).toBe("number");
        expect(typeof dashboard.classification.integration_gap).toBe("number");
        expect(typeof dashboard.classification.principle_rejection).toBe("number");
        expect(typeof dashboard.classification.unknown).toBe("number");

        // Total should add up.
        const classTotal = Object.values(dashboard.classification).reduce((a, b) => a + b, 0);
        expect(classTotal).toBe(dashboard.totalScenarios);

        // perStageFailures is an object (may be empty if everything matched).
        expect(typeof dashboard.perStageFailures).toBe("object");
        for (const [, data] of Object.entries(dashboard.perStageFailures)) {
          expect(typeof data.count).toBe("number");
          expect(Array.isArray(data.topReasons)).toBe(true);
          expect(data.topReasons.length).toBeLessThanOrEqual(3);
        }
      },
    );

    it(
      "adv-missing-file scenario produces expected_failure or integration_gap (locate fails)",
      { timeout: 60_000 },
      async () => {
        const results = await runSweep([advMissingFile]);
        const r = results[0];

        // The locate stage should fail (file doesn't exist in SAST DB).
        // Classification should be expected_failure or integration_gap depending on
        // whether the actual failure stage matches the expected.fails.stage.
        expect(["expected_failure", "integration_gap", "match"]).toContain(r.classification);

        // Either way, the outcome should not be "applied".
        expect(r.actual.outcome).not.toBe("applied");
      },
    );
  },
);
