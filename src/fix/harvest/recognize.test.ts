/**
 * Tests for harvest/recognize.ts. Build a synthetic principle library +
 * synthetic HarvestCandidates against an in-memory scratch principles dir.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { recognizeCandidate, parseDiffDirtyLines } from "./recognize.js";
import type { HarvestCandidate } from "./extractBugs.js";

// A minimal candidate with one buggy file containing a division-by-zero shape.
function makeCandidate(buggyFiles: Record<string, string>, diff?: string): HarvestCandidate {
  // Default diff: pretend the entire content was added in one hunk so every
  // line counts as dirty. Tests that care about locus constraints pass an
  // explicit diff with a narrower hunk range.
  const synthDiff = diff ?? Object.entries(buggyFiles).map(([path, content]) => {
    const lineCount = content.split("\n").length;
    return `diff --git a/${path} b/${path}\n--- a/${path}\n+++ b/${path}\n@@ -1,${lineCount} +1,${lineCount} @@\n`;
  }).join("");
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

describe("parseDiffDirtyLines", () => {
  it("extracts hunk ranges from a unified diff with neighborhood expansion", () => {
    const diff =
      `diff --git a/lib/foo.js b/lib/foo.js\n` +
      `--- a/lib/foo.js\n+++ b/lib/foo.js\n` +
      `@@ -10,3 +10,4 @@\n` +
      ` line 10\n+inserted\n line 11\n line 12\n`;
    const map = parseDiffDirtyLines(diff);
    const ranges = map.get("lib/foo.js");
    expect(ranges).toBeDefined();
    expect(ranges![0]).toEqual([7, 15]); // start - 3, (start + len - 1) + 3
  });

  it("handles multiple files and multiple hunks", () => {
    const diff =
      `diff --git a/a.js b/a.js\n--- a/a.js\n+++ b/a.js\n@@ -5,1 +5,1 @@\n@@ -20,2 +20,2 @@\n` +
      `diff --git a/b.js b/b.js\n--- a/b.js\n+++ b/b.js\n@@ -100,1 +100,1 @@\n`;
    const map = parseDiffDirtyLines(diff);
    expect(map.get("a.js")!.length).toBe(2);
    expect(map.get("b.js")!.length).toBe(1);
    expect(map.get("b.js")![0]).toEqual([97, 103]);
  });

  it("treats a hunk header with no length as length 1", () => {
    const diff =
      `diff --git a/a.js b/a.js\n--- a/a.js\n+++ b/a.js\n@@ -42 +42 @@\n`;
    const map = parseDiffDirtyLines(diff);
    expect(map.get("a.js")![0]).toEqual([39, 45]);
  });
});

describe("recognizeCandidate", () => {
  let principlesDir: string;
  let scratchParent: string;

  beforeEach(() => {
    principlesDir = mkdtempSync(join(tmpdir(), "harvest-recognize-lib-"));
    scratchParent = mkdtempSync(join(tmpdir(), "harvest-recognize-scratch-"));
  });

  afterEach(() => {
    rmSync(principlesDir, { recursive: true, force: true });
    rmSync(scratchParent, { recursive: true, force: true });
  });

  it("returns recognized=false when the library is empty", () => {
    const c = makeCandidate({ "src/foo.ts": "const x = 1 / 0;\n" });
    const r = recognizeCandidate(c, { principlesDir, scratchParent });
    expect(r.recognized).toBe(false);
    expect(r.matches).toEqual([]);
    expect(r.principlesEvaluated).toBe(0);
  }, 30_000);

  it("recognizes a candidate when a library principle matches its buggy file", () => {
    // Library principle: any arithmetic division.
    const dsl = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation {
    at $x
    captures { site: $x }
    message "division — possibly by zero"
  }
}`;
    writeFileSync(join(principlesDir, "div.dsl"), dsl);

    const c = makeCandidate({
      "src/calc.ts": "function bug() { return 100 / 0; }\n",
    });
    const r = recognizeCandidate(c, { principlesDir, scratchParent });

    expect(r.principlesEvaluated).toBe(1);
    expect(r.principleErrors).toBe(0);
    expect(r.recognized).toBe(true);
    expect(r.matches.length).toBeGreaterThan(0);
    expect(r.matches[0]!.principleName).toBe("div_by_zero");
    expect(r.matches[0]!.filePath).toBe("src/calc.ts");
    expect(r.matches[0]!.line).toBeGreaterThan(0);
  }, 60_000);

  it("does not recognize a candidate whose buggy file lacks the matched shape", () => {
    const dsl = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation {
    at $x
    captures { site: $x }
    message "division — possibly by zero"
  }
}`;
    writeFileSync(join(principlesDir, "div.dsl"), dsl);

    // No division — the principle should NOT match.
    const c = makeCandidate({
      "src/calc.ts": "function ok() { return 1 + 2; }\n",
    });
    const r = recognizeCandidate(c, { principlesDir, scratchParent });

    expect(r.principlesEvaluated).toBe(1);
    expect(r.recognized).toBe(false);
    expect(r.matches).toEqual([]);
  }, 60_000);

  it("counts a malformed DSL as principleErrors and continues", () => {
    writeFileSync(join(principlesDir, "broken.dsl"), `not valid dsl &&!`);
    const valid = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation { at $x captures { site: $x } message "div" }
}`;
    writeFileSync(join(principlesDir, "valid.dsl"), valid);

    const c = makeCandidate({
      "src/calc.ts": "const z = 1 / 0;\n",
    });
    const r = recognizeCandidate(c, { principlesDir, scratchParent });

    expect(r.principlesEvaluated).toBe(2);
    expect(r.principleErrors).toBe(1);
    expect(r.recognized).toBe(true); // valid principle still fired
  }, 60_000);

  it("does NOT recognize when the principle matches outside the diff's locus", () => {
    // The library principle matches any division. The buggy file has a
    // division on line 1, but the diff hunk is at line 100 — far from it.
    // A naive "matches anywhere in any changed file" recognizer would say
    // recognized=true; the diff-locus-aware one must say false.
    const dsl = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation { at $x captures { site: $x } message "div" }
}`;
    writeFileSync(join(principlesDir, "div.dsl"), dsl);

    const buggyContent = "const z = 1 / 0;\n" + "// padding\n".repeat(120);
    const c = makeCandidate(
      { "src/calc.ts": buggyContent },
      // Hunk at lines 100-105: nowhere near the division on line 1.
      `diff --git a/src/calc.ts b/src/calc.ts\n--- a/src/calc.ts\n+++ b/src/calc.ts\n@@ -100,5 +100,5 @@\n`,
    );
    const r = recognizeCandidate(c, { principlesDir, scratchParent });
    expect(r.recognized).toBe(false);
  }, 60_000);

  it("recognizes when the principle matches within the diff locus (with neighborhood slack)", () => {
    const dsl = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation { at $x captures { site: $x } message "div" }
}`;
    writeFileSync(join(principlesDir, "div.dsl"), dsl);

    // Padding 1-99, division on line 100, more padding. Hunk at 100-102.
    const buggyContent =
      "// pad\n".repeat(99) + "const z = 1 / 0;\n" + "// pad\n".repeat(20);
    const c = makeCandidate(
      { "src/calc.ts": buggyContent },
      `diff --git a/src/calc.ts b/src/calc.ts\n--- a/src/calc.ts\n+++ b/src/calc.ts\n@@ -100,3 +100,3 @@\n`,
    );
    const r = recognizeCandidate(c, { principlesDir, scratchParent });
    expect(r.recognized).toBe(true);
    expect(r.matches[0]!.line).toBe(100);
  }, 60_000);

  it("skips test-path files when materializing buggy files", () => {
    const dsl = `principle div_by_zero {
  match $x: node where arithmetic.op == "/"
  report violation { at $x captures { site: $x } message "div" }
}`;
    writeFileSync(join(principlesDir, "div.dsl"), dsl);

    // The division is in test/, which the recognizer skips.
    const c = makeCandidate({
      "test/foo.test.ts": "const z = 1 / 0;\n",
    });
    const r = recognizeCandidate(c, { principlesDir, scratchParent });

    expect(r.filesIndexed).toBe(0);
    expect(r.recognized).toBe(false);
  }, 30_000);
});
