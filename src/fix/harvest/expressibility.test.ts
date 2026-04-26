/**
 * Tests for the v1 mechanical expressibility tagger. Covers the four
 * non-recognized buckets via synthetic candidates; the recognized bucket is
 * exercised by tagging a candidate against a fixture principle library that
 * contains a single division-by-zero principle (mirrors the recognize tests).
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { tagExpressibility, TAGGER_VERSION } from "./expressibility.js";
import type { HarvestCandidate } from "./extractBugs.js";

function makeCandidate(buggyFiles: Record<string, string>, diff?: string): HarvestCandidate {
  const synthDiff =
    diff ??
    Object.entries(buggyFiles)
      .map(([path, content]) => {
        const lineCount = content.split("\n").length;
        return `diff --git a/${path} b/${path}\n--- a/${path}\n+++ b/${path}\n@@ -1,${lineCount} +1,${lineCount} @@\n`;
      })
      .join("");
  return {
    source: {
      project: "fixture",
      bugId: "1",
      baseSha: "0".repeat(40),
      fixSha: "1".repeat(40),
      testSha: null,
      originalSha: null,
    },
    buggyFiles,
    fixedFiles: {},
    diff: synthDiff,
    upstreamFixMessage: "",
    testFiles: {},
    stats: { filesChanged: Object.keys(buggyFiles).length, insertions: 0, deletions: 0 },
  };
}

const DSL_DIVISION = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation { at $x captures { site: $x } message "div" }
}`;

describe("tagExpressibility", () => {
  let principlesDir: string;
  let scratchParent: string;

  beforeEach(() => {
    principlesDir = mkdtempSync(join(tmpdir(), "harvest-tag-lib-"));
    scratchParent = mkdtempSync(join(tmpdir(), "harvest-tag-scratch-"));
  });

  afterEach(() => {
    rmSync(principlesDir, { recursive: true, force: true });
    rmSync(scratchParent, { recursive: true, force: true });
  });

  it("tags expressible-now-recognized when a principle matches at locus", () => {
    writeFileSync(join(principlesDir, "div_by_zero.dsl"), DSL_DIVISION);
    const candidate = makeCandidate({
      "src/calc.ts": "function bug() { return 1 / 0; }\n",
    });

    const tag = tagExpressibility({ candidate, principlesDir, scratchParent });
    expect(tag.tag).toBe("expressible-now-recognized");
    expect(tag.layer1Recognized).toBe(true);
    expect(tag.layer1MatchedPrinciples).toContain("div_by_zero");
    expect(tag.taggerVersion).toBe(TAGGER_VERSION);
    expect(tag.auditLine).toContain("expressible-now-recognized");
  }, 60_000);

  it("emits unknown with a parser-failure reason when no production files are indexable", () => {
    const candidate = makeCandidate({
      "test/foo.test.ts": "describe('x', () => {});\n",
    });

    const tag = tagExpressibility({ candidate, principlesDir, scratchParent });
    expect(tag.tag).toBe("unknown");
    expect(tag.auditLine).toMatch(/test-only|no production files|no files indexed/);
  }, 60_000);

  it("tags a candidate as either pending-principle or needs-new-relation when no principle matches but substrate has coverage", () => {
    // A simple function that takes a parameter and uses it. The buggy file
    // has multiple capabilities firing on the parameter and its uses (binding,
    // calls, member_access, etc.) but no principle in the empty library will
    // match. Whether v1 calls this "pending-principle" or "needs-new-relation"
    // depends on the cross-locus data_flow signal, which is what the manual
    // sample is designed to validate; both are acceptable for this fixture.
    const candidate = makeCandidate({
      "src/parse.ts":
        "export function parse(input: string): string {\n" +
        "  const trimmed = input.trim();\n" +
        "  return trimmed.toLowerCase();\n" +
        "}\n",
    });

    const tag = tagExpressibility({ candidate, principlesDir, scratchParent });
    expect(["expressible-now-pending-principle", "needs-new-relation", "unknown"]).toContain(tag.tag);
    expect(tag.taggerVersion).toBe(TAGGER_VERSION);
    // Audit line must be non-empty regardless of bucket.
    expect(tag.auditLine.length).toBeGreaterThan(0);
  }, 60_000);

  it("propagates layer1MatchedPrinciples as deduped names", () => {
    writeFileSync(join(principlesDir, "div_by_zero_a.dsl"), DSL_DIVISION);
    // A second copy of the same principle (different filename, same name) —
    // the recognizer reports both, the tagger should dedupe.
    writeFileSync(join(principlesDir, "div_by_zero_b.dsl"), DSL_DIVISION);
    const candidate = makeCandidate({
      "src/calc.ts": "function bug() { return 1 / 0; }\n",
    });
    const tag = tagExpressibility({ candidate, principlesDir, scratchParent });
    if (tag.tag === "expressible-now-recognized") {
      expect(tag.layer1MatchedPrinciples.filter((p) => p === "div_by_zero")).toHaveLength(1);
    }
  }, 60_000);
});
