/**
 * Contract validator smoke tests. Confirms validateIntentReport accepts a
 * minimal well-formed report and rejects the load-bearing malformations
 * (bad source, bad lineRange, bad validationStatus, missing fields).
 */

import { describe, it, expect } from "vitest";
import {
  validateIntentReport,
  IntentReportValidationError,
} from "./intentReport.js";

const wellFormed = {
  source: "retrospective" as const,
  trigger: {
    kind: "commit" as const,
    ref: "abc123",
    diff: "--- a\n+++ b\n",
    commitMessage: "fix off-by-one",
  },
  intents: [
    {
      filePath: "src/foo.ts",
      lineRange: [10, 12] as [number, number],
      intent: "ensure k > 0 before division",
      hasRegressionTest: false,
      testGenerationOpportunity: true,
      constraintCandidate: {
        smtSketch: "(assert (> k 0))",
        kind: "bound",
        validationStatus: "candidate" as const,
      },
    },
  ],
  outputBundle: {
    patch: null,
    addedTests: [],
    constraintArtifact: null,
  },
};

describe("validateIntentReport", () => {
  it("accepts a minimal well-formed report", () => {
    expect(() => validateIntentReport(wellFormed)).not.toThrow();
  });

  it("rejects bad source", () => {
    const bad = { ...wellFormed, source: "sideways" };
    expect(() => validateIntentReport(bad)).toThrow(IntentReportValidationError);
  });

  it("rejects inverted lineRange", () => {
    const bad = {
      ...wellFormed,
      intents: [{ ...wellFormed.intents[0], lineRange: [12, 10] }],
    };
    expect(() => validateIntentReport(bad)).toThrow(/lineRange/);
  });

  it("rejects 0-indexed lineRange", () => {
    const bad = {
      ...wellFormed,
      intents: [{ ...wellFormed.intents[0], lineRange: [0, 5] }],
    };
    expect(() => validateIntentReport(bad)).toThrow(/1-indexed/);
  });

  it("rejects unknown validationStatus", () => {
    const bad = {
      ...wellFormed,
      intents: [
        {
          ...wellFormed.intents[0],
          constraintCandidate: {
            ...wellFormed.intents[0].constraintCandidate!,
            validationStatus: "approved-by-vibes",
          },
        },
      ],
    };
    expect(() => validateIntentReport(bad)).toThrow(/validationStatus/);
  });

  it("accepts constraintCandidate: null (intent without SMT shape)", () => {
    const ok = {
      ...wellFormed,
      intents: [{ ...wellFormed.intents[0], constraintCandidate: null }],
    };
    expect(() => validateIntentReport(ok)).not.toThrow();
  });

  it("reports a path-style locator on failure", () => {
    const bad = {
      ...wellFormed,
      intents: [
        wellFormed.intents[0],
        { ...wellFormed.intents[0], lineRange: [-1, 5] },
      ],
    };
    try {
      validateIntentReport(bad);
      throw new Error("expected to throw");
    } catch (e) {
      expect(e).toBeInstanceOf(IntentReportValidationError);
      expect((e as IntentReportValidationError).path).toBe("intents[1].lineRange[0]");
    }
  });
});
