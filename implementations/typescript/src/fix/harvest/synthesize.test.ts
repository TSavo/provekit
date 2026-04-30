import { describe, it, expect } from "vitest";
import {
  synthesizeBugSignal,
  synthesizeFixCandidate,
  buildInvariantSynthesisPrompt,
} from "./synthesize.js";
import type { HarvestCandidate } from "./extractBugs.js";

function makeCandidate(overrides: Partial<HarvestCandidate> = {}): HarvestCandidate {
  return {
    source: {
      project: "express",
      bugId: "1",
      baseSha: "0".repeat(40),
      fixSha: "1".repeat(40),
      testSha: null,
      originalSha: null,
    },
    buggyFiles: { "lib/foo.js": "const z = 1 / 0;\n" },
    fixedFiles: { "lib/foo.js": "if (d !== 0) { const z = 1 / d; }\n" },
    diff: `diff --git a/lib/foo.js b/lib/foo.js
--- a/lib/foo.js
+++ b/lib/foo.js
@@ -10,3 +10,3 @@
`,
    upstreamFixMessage: "Fix division by zero in foo()\n\nReported by user; reproduces with d=0.",
    testFiles: {},
    stats: { filesChanged: 1, insertions: 1, deletions: 1 },
    ...overrides,
  };
}

describe("synthesizeBugSignal", () => {
  it("uses the first non-empty line of the upstream message as summary", () => {
    const c = makeCandidate();
    const signal = synthesizeBugSignal(c);
    expect(signal.summary).toBe("Fix division by zero in foo()");
    expect(signal.failureDescription).toContain("Reported by user");
    expect(signal.rawText).toBe(c.upstreamFixMessage.trim());
    expect(signal.source).toBe("harvest");
  });

  it("falls back to the summary when there is no body", () => {
    const c = makeCandidate({ upstreamFixMessage: "One-liner fix" });
    const signal = synthesizeBugSignal(c);
    expect(signal.summary).toBe("One-liner fix");
    expect(signal.failureDescription).toBe("One-liner fix");
  });

  it("emits one codeReference per production file with the first hunk's line", () => {
    const c = makeCandidate({
      fixedFiles: {
        "lib/foo.js": "...",
        "lib/bar.js": "...",
        "test/foo.test.js": "...", // skipped
      },
      diff: `diff --git a/lib/foo.js b/lib/foo.js
--- a/lib/foo.js
+++ b/lib/foo.js
@@ -10,1 +10,1 @@
diff --git a/lib/bar.js b/lib/bar.js
--- a/lib/bar.js
+++ b/lib/bar.js
@@ -42,1 +42,1 @@
diff --git a/test/foo.test.js b/test/foo.test.js
--- a/test/foo.test.js
+++ b/test/foo.test.js
@@ -1,1 +1,1 @@
`,
    });
    const signal = synthesizeBugSignal(c);
    const paths = signal.codeReferences.map((r) => r.file).sort();
    expect(paths).toEqual(["lib/bar.js", "lib/foo.js"]);
    const fooRef = signal.codeReferences.find((r) => r.file === "lib/foo.js");
    expect(fooRef!.line).toBe(10);
    const barRef = signal.codeReferences.find((r) => r.file === "lib/bar.js");
    expect(barRef!.line).toBe(42);
  });

  it("handles empty upstream message gracefully", () => {
    const c = makeCandidate({ upstreamFixMessage: "" });
    const signal = synthesizeBugSignal(c);
    expect(signal.summary).toBe("");
    expect(signal.rawText).toBe("");
  });
});

describe("synthesizeFixCandidate", () => {
  it("builds a CodePatch from production fixedFiles, skipping tests", () => {
    const c = makeCandidate({
      fixedFiles: {
        "lib/foo.js": "fix-foo\n",
        "test/foo.test.js": "fix-test\n",
      },
    });
    const fc = synthesizeFixCandidate(c);
    expect(fc.patch.fileEdits).toHaveLength(1);
    expect(fc.patch.fileEdits[0]!.file).toBe("lib/foo.js");
    expect(fc.patch.fileEdits[0]!.newContent).toBe("fix-foo\n");
    expect(fc.patch.description).toContain("Fix division by zero");
    expect(fc.invariantHoldsUnderOverlay).toBe(true);
    expect(fc.overlayZ3Verdict).toBe("ground-truth");
  });

  it("produces a stable description fallback when commit message is empty", () => {
    const c = makeCandidate({ upstreamFixMessage: "", fixedFiles: { "lib/x.js": "y\n" } });
    const fc = synthesizeFixCandidate(c);
    expect(fc.patch.description).toContain("express bug-1");
  });
});

describe("buildInvariantSynthesisPrompt", () => {
  it("includes commit message, diff, and JSON schema instruction", () => {
    const c = makeCandidate();
    const prompt = buildInvariantSynthesisPrompt(c);
    expect(prompt).toContain("Fix division by zero");
    expect(prompt).toContain("diff --git");
    expect(prompt).toContain("smt_violation_assertion");
    expect(prompt).toContain("kind");
  });

  it("truncates very large diffs", () => {
    const huge = "x".repeat(10000);
    const c = makeCandidate({ diff: huge });
    const prompt = buildInvariantSynthesisPrompt(c);
    expect(prompt).toContain("(truncated");
    expect(prompt.length).toBeLessThan(huge.length);
  });
});
