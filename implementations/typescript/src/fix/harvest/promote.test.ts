import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { promoteStagedRecord } from "./promote.js";
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

describe("promoteStagedRecord", () => {
  let tmpRoot: string;
  let stagingDir: string;
  let principlesDir: string;
  let scratchParent: string;

  beforeEach(() => {
    tmpRoot = mkdtempSync(join(tmpdir(), "harvest-promote-"));
    stagingDir = join(tmpRoot, "staging");
    principlesDir = join(tmpRoot, "principles");
    scratchParent = join(tmpRoot, "scratch");
    mkdirSync(stagingDir, { recursive: true });
    mkdirSync(scratchParent, { recursive: true });
  });

  afterEach(() => {
    rmSync(tmpRoot, { recursive: true, force: true });
  });

  function writeStaged(name: string, dsl: string): string {
    const path = join(stagingDir, name + ".json");
    writeFileSync(path, JSON.stringify({
      candidate: {
        source: { project: "fixture", bugId: "1", baseSha: "0".repeat(40), fixSha: "1".repeat(40), testSha: null, originalSha: null },
        upstreamFixMessage: "fix",
        stats: { filesChanged: 1, insertions: 1, deletions: 1 },
        diff: `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -1,1 +1,1 @@
`,
      },
      outcome: { kind: "ok" },
      principles: [{ kind: "principle", name: "div_by_zero", bugClassId: "div-by-zero", dslSource: dsl }],
    }, null, 2), "utf-8");
    return path;
  }

  it("promotes a principle that passes validation: writes .dsl + .json into the library", () => {
    const stagedPath = writeStaged("ok", DSL_DIVISION);
    const source = makeCandidate({
      buggyFiles: { "src/calc.ts": "function bug() { return 1 / 0; }\n" },
      diff: `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -1,1 +1,1 @@
`,
    });

    const result = promoteStagedRecord({
      stagedPath,
      source,
      cohort: [],
      principlesDir,
      scratchParent,
    });

    expect(result.promoted).toBe(1);
    expect(result.quarantined).toBe(0);
    // Task #134: promoted principles land in universal/ unless a
    // language tag is present on the staged record.
    expect(existsSync(join(principlesDir, "universal", "div_by_zero.dsl"))).toBe(true);
    expect(existsSync(join(principlesDir, "universal", "div_by_zero.json"))).toBe(true);

    const dsl = readFileSync(join(principlesDir, "universal", "div_by_zero.dsl"), "utf-8");
    expect(dsl).toContain('arithmetic.op == "/"');

    const json = JSON.parse(readFileSync(join(principlesDir, "universal", "div_by_zero.json"), "utf-8"));
    expect(json.id).toBe("div_by_zero");
    expect(json.bug_class_id).toBe("div-by-zero");
    expect(json.provenance[0].source).toBe("harvest");
    expect(json.provenance[0].projectId).toBe("fixture");
    expect(json.provenance[0].bugId).toBe("1");
  }, 60_000);

  it("quarantines a principle whose positive check fails", () => {
    // Bug exists in source file but the diff hunk is at line 100 — far from
    // the line-1 division. Locus-aware validation rejects.
    const padding = "// pad\n".repeat(120);
    const stagedPath = writeStaged("bad-locus", DSL_DIVISION);
    // Override the staged record's diff to point hunk at line 100.
    const r = JSON.parse(readFileSync(stagedPath, "utf-8"));
    r.candidate.diff = `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -100,1 +100,1 @@
`;
    writeFileSync(stagedPath, JSON.stringify(r, null, 2), "utf-8");

    const source = makeCandidate({
      buggyFiles: { "src/calc.ts": "const z = 1 / 0;\n" + padding },
      diff: r.candidate.diff,
    });

    const result = promoteStagedRecord({
      stagedPath,
      source,
      cohort: [],
      principlesDir,
      scratchParent,
    });

    expect(result.promoted).toBe(0);
    expect(result.quarantined).toBe(1);
    expect(existsSync(join(principlesDir, "universal", "div_by_zero.dsl"))).toBe(false);

    // Audit trail in the staged file:
    const audited = JSON.parse(readFileSync(stagedPath, "utf-8"));
    expect(audited.validation.quarantined).toBe(1);
    expect(audited.validation.perPrinciple[0].promoted).toBe(false);
  }, 60_000);

  it("merges provenance when the principle file already exists", () => {
    // First promotion.
    const staged1 = writeStaged("first", DSL_DIVISION);
    const source = makeCandidate({
      buggyFiles: { "src/calc.ts": "const z = 1 / 0;\n" },
      diff: `diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -1,1 +1,1 @@
`,
    });
    promoteStagedRecord({ stagedPath: staged1, source, cohort: [], principlesDir, scratchParent });

    // Second promotion (different bugId — same principle name).
    const staged2 = writeStaged("second", DSL_DIVISION);
    const r2 = JSON.parse(readFileSync(staged2, "utf-8"));
    r2.candidate.source.bugId = "2";
    writeFileSync(staged2, JSON.stringify(r2, null, 2), "utf-8");
    const source2 = makeCandidate({
      source: { ...source.source, bugId: "2" },
      buggyFiles: source.buggyFiles,
      diff: source.diff,
    });
    promoteStagedRecord({ stagedPath: staged2, source: source2, cohort: [], principlesDir, scratchParent });

    const json = JSON.parse(readFileSync(join(principlesDir, "universal", "div_by_zero.json"), "utf-8"));
    expect(json.provenance).toHaveLength(2);
    const bugIds = json.provenance.map((p: any) => p.bugId).sort();
    expect(bugIds).toEqual(["1", "2"]);
  }, 60_000);
});
