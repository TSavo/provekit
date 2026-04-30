import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { validateStagedPrinciple } from "./validate.js";
import type { HarvestCandidate } from "./extractBugs.js";

function makeCandidate(overrides: Partial<HarvestCandidate> = {}): HarvestCandidate {
  return {
    source: {
      project: "fixture",
      bugId: "1",
      baseSha: "0".repeat(40),
      fixSha: "1".repeat(40),
      testSha: null,
      originalSha: null,
    },
    buggyFiles: {},
    fixedFiles: {},
    diff: "",
    upstreamFixMessage: "",
    testFiles: {},
    stats: { filesChanged: 0, insertions: 0, deletions: 0 },
    ...overrides,
  };
}

const DSL_DIVISION = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation { at $x captures { site: $x } message "div" }
}`;

describe("validateStagedPrinciple", () => {
  let scratchParent: string;

  beforeEach(() => {
    scratchParent = mkdtempSync(join(tmpdir(), "harvest-validate-"));
  });

  afterEach(() => {
    rmSync(scratchParent, { recursive: true, force: true });
  });

  it("passes when principle matches its own bug at locus and cohort match rate is low", () => {
    const source = makeCandidate({
      buggyFiles: { "src/calc.ts": "function bug() { return 100 / 0; }\n" },
      diff: `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -1,1 +1,1 @@
`,
    });
    // Cohort with no division operator → no false positives.
    const cohort = [
      makeCandidate({
        source: { project: "fixture", bugId: "2", baseSha: "0".repeat(40), fixSha: "0".repeat(40), testSha: null, originalSha: null },
        buggyFiles: { "src/other.ts": "function ok() { return 1 + 2; }\n" },
      }),
      makeCandidate({
        source: { project: "fixture", bugId: "3", baseSha: "0".repeat(40), fixSha: "0".repeat(40), testSha: null, originalSha: null },
        buggyFiles: { "src/other2.ts": "const x = 'no division here';\n" },
      }),
    ];

    const result = validateStagedPrinciple({
      dslSource: DSL_DIVISION,
      source,
      cohort,
      scratchParent,
    });
    expect(result.positivePass).toBe(true);
    expect(result.cohortMatchCount).toBe(0);
    expect(result.passed).toBe(true);
  }, 60_000);

  it("fails the positive check when principle does not match its own bug at the locus", () => {
    // The bug has division, but the diff hunk is far from line 1.
    const padding = "// pad\n".repeat(120);
    const source = makeCandidate({
      buggyFiles: { "src/calc.ts": "const z = 1 / 0;\n" + padding },
      diff: `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -100,1 +100,1 @@
`,
    });

    const result = validateStagedPrinciple({
      dslSource: DSL_DIVISION,
      source,
      cohort: [],
      scratchParent,
    });
    expect(result.positivePass).toBe(false);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("did not match its own source bug");
  }, 60_000);

  it("fails when cohort match rate exceeds threshold", () => {
    const source = makeCandidate({
      buggyFiles: { "src/calc.ts": "const z = 1 / d;\n" },
      diff: `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -1,1 +1,1 @@
`,
    });
    // Three cohort candidates ALL containing division → 100% false positive.
    const cohort = [
      makeCandidate({
        source: { project: "fixture", bugId: "2", baseSha: "0".repeat(40), fixSha: "0".repeat(40), testSha: null, originalSha: null },
        buggyFiles: { "src/a.ts": "const a = 1 / 2;\n" },
      }),
      makeCandidate({
        source: { project: "fixture", bugId: "3", baseSha: "0".repeat(40), fixSha: "0".repeat(40), testSha: null, originalSha: null },
        buggyFiles: { "src/b.ts": "const b = 4 / 2;\n" },
      }),
      makeCandidate({
        source: { project: "fixture", bugId: "4", baseSha: "0".repeat(40), fixSha: "0".repeat(40), testSha: null, originalSha: null },
        buggyFiles: { "src/c.ts": "const c = 9 / 3;\n" },
      }),
    ];

    const result = validateStagedPrinciple({
      dslSource: DSL_DIVISION,
      source,
      cohort,
      maxCohortMatchRate: 0.3,
      scratchParent,
    });
    expect(result.positivePass).toBe(true);
    expect(result.cohortMatchCount).toBe(3);
    expect(result.cohortMatchRate).toBe(1);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("cohort match rate");
  }, 90_000);

  it("returns positivePass=false when DSL source is malformed", () => {
    const source = makeCandidate({
      buggyFiles: { "src/calc.ts": "const z = 1 / 0;\n" },
      diff: `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -1,1 +1,1 @@
`,
    });

    const result = validateStagedPrinciple({
      dslSource: `not valid && dsl`,
      source,
      cohort: [],
      scratchParent,
    });
    expect(result.positivePass).toBe(false);
    expect(result.passed).toBe(false);
  }, 30_000);
});
