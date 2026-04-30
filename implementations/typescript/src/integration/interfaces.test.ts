/**
 * Type guards and Artifact-union shape tests for src/integration/interfaces.ts.
 *
 * The interfaces themselves are pure types (no runtime), but the type guards
 * are runtime values that integrators rely on to discriminate the artifact
 * union. Cover them.
 */

import { describe, it, expect } from "vitest";
import {
  isInvariantArtifact,
  isPatchArtifact,
  isRegressionTestArtifact,
  isPrincipleArtifact,
  isIntentReportArtifact,
  isBundleArtifact,
  isOracleReportArtifact,
  type Artifact,
} from "./interfaces.js";

describe("Artifact type guards", () => {
  it("isInvariantArtifact matches kind=invariant only", () => {
    const a: Artifact = {
      kind: "invariant",
      claim: {} as never,
      signal: {} as never,
      locus: {} as never,
      test: null,
      patchSha: null,
    };
    expect(isInvariantArtifact(a)).toBe(true);
    expect(isPatchArtifact(a)).toBe(false);
    expect(isRegressionTestArtifact(a)).toBe(false);
    expect(isPrincipleArtifact(a)).toBe(false);
    expect(isIntentReportArtifact(a)).toBe(false);
    expect(isBundleArtifact(a)).toBe(false);
    expect(isOracleReportArtifact(a)).toBe(false);
  });

  it("isPatchArtifact matches kind=patch only", () => {
    const a: Artifact = {
      kind: "patch",
      patch: { fileEdits: [], description: "" },
      rationale: "test",
    };
    expect(isPatchArtifact(a)).toBe(true);
    expect(isInvariantArtifact(a)).toBe(false);
  });

  it("isRegressionTestArtifact matches kind=regression_test only", () => {
    const a: Artifact = {
      kind: "regression_test",
      test: {} as never,
    };
    expect(isRegressionTestArtifact(a)).toBe(true);
    expect(isInvariantArtifact(a)).toBe(false);
  });

  it("isPrincipleArtifact matches kind=principle only", () => {
    const a: Artifact = {
      kind: "principle",
      principle: {} as never,
    };
    expect(isPrincipleArtifact(a)).toBe(true);
    expect(isPatchArtifact(a)).toBe(false);
  });

  it("isIntentReportArtifact matches kind=intent_report only", () => {
    const a: Artifact = {
      kind: "intent_report",
      report: {} as never,
    };
    expect(isIntentReportArtifact(a)).toBe(true);
    expect(isInvariantArtifact(a)).toBe(false);
  });

  it("isBundleArtifact matches kind=bundle only", () => {
    const a: Artifact = {
      kind: "bundle",
      bundle: {} as never,
    };
    expect(isBundleArtifact(a)).toBe(true);
    expect(isPrincipleArtifact(a)).toBe(false);
  });

  it("isOracleReportArtifact matches kind=oracle_report only", () => {
    const a: Artifact = {
      kind: "oracle_report",
      oracleId: "1",
      verdict: "pass",
      detail: "x",
    };
    expect(isOracleReportArtifact(a)).toBe(true);
    expect(isBundleArtifact(a)).toBe(false);
  });

  it("type guards narrow correctly inside conditionals", () => {
    const stream: Artifact[] = [
      {
        kind: "patch",
        patch: { fileEdits: [{ file: "a.ts", newContent: "x" }], description: "" },
        rationale: "ok",
      },
      {
        kind: "oracle_report",
        oracleId: "9a",
        verdict: "pass",
        detail: "test passes",
      },
    ];

    let patchCount = 0;
    let oracleCount = 0;
    for (const a of stream) {
      if (isPatchArtifact(a)) {
        // a.patch.fileEdits is reachable because of narrowing
        expect(a.patch.fileEdits.length).toBe(1);
        patchCount++;
      } else if (isOracleReportArtifact(a)) {
        expect(a.verdict).toBe("pass");
        oracleCount++;
      }
    }
    expect(patchCount).toBe(1);
    expect(oracleCount).toBe(1);
  });
});
