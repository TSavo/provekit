/**
 * Tests for discover.ts. Use StubLLMProvider so no real LLM calls happen.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { discoverPrinciple } from "./discover.js";
import { StubLLMProvider } from "../types.js";
import type { HarvestCandidate } from "./extractBugs.js";

function makeCandidate(): HarvestCandidate {
  return {
    source: {
      project: "fixture",
      bugId: "1",
      baseSha: "0".repeat(40),
      fixSha: "1".repeat(40),
      testSha: null,
      originalSha: null,
    },
    buggyFiles: { "lib/foo.ts": "function bug(d: number) { return 1 / d; }\n" },
    fixedFiles: { "lib/foo.ts": "function bug(d: number) { if (d === 0) return null; return 1 / d; }\n" },
    diff: `diff --git a/lib/foo.ts b/lib/foo.ts
--- a/lib/foo.ts
+++ b/lib/foo.ts
@@ -1,1 +1,1 @@
`,
    upstreamFixMessage: "Fix division by zero in bug()",
    testFiles: {},
    stats: { filesChanged: 1, insertions: 1, deletions: 1 },
  };
}

describe("discoverPrinciple", () => {
  let scratchParent: string;

  beforeEach(() => {
    scratchParent = mkdtempSync(join(tmpdir(), "harvest-discover-test-"));
  });

  afterEach(() => {
    rmSync(scratchParent, { recursive: true, force: true });
  });

  it("returns invariant_synthesis_failed when the LLM returns malformed JSON", async () => {
    const llm = new StubLLMProvider(new Map([["division", "not valid json"]]));
    const result = await discoverPrinciple({
      candidate: makeCandidate(),
      llm,
      scratchParent,
    });
    expect(result.outcome.kind).toBe("invariant_synthesis_failed");
    expect(result.synthesizedInvariant).toBeNull();
  }, 30_000);

  it("returns invariant_synthesis_failed when JSON lacks required fields", async () => {
    const llm = new StubLLMProvider(
      new Map([["division", JSON.stringify({ description: "ok but no SMT" })]]),
    );
    const result = await discoverPrinciple({
      candidate: makeCandidate(),
      llm,
      scratchParent,
    });
    expect(result.outcome.kind).toBe("invariant_synthesis_failed");
    expect(result.synthesizedInvariant).toBeNull();
  }, 30_000);

  it("synthesizes invariant + advances to C6, returning non_codifiable when LLM punts", async () => {
    const invariantResponse = JSON.stringify({
      description: "denominator must not be zero",
      kind: "arithmetic",
      smt_declarations: ["(declare-const d Int)"],
      smt_violation_assertion: "(assert (= d 0))",
      bindings: [{ smt_constant: "d", source_expr: "d", sort: "Int" }],
    });
    // The C6 principle proposer in tryExistingCapabilities uses a different
    // prompt — we stub a non_codifiable response keyed off the principle
    // proposer prompt's distinguishing fragment ("denominator must not be zero"
    // appears in both prompts; the proposer prompt also has "static-analysis
    // rule author" as a unique marker).
    const llm = new StubLLMProvider(
      new Map<string, string>([
        ["formal-verification expert", invariantResponse],
        ["static-analysis rule author", JSON.stringify({ kind: "non_codifiable", reason: "too project-specific" })],
      ]),
    );
    const result = await discoverPrinciple({
      candidate: makeCandidate(),
      llm,
      scratchParent,
    });
    expect(result.synthesizedInvariant).not.toBeNull();
    expect(result.synthesizedInvariant!.description).toContain("denominator must not be zero");
    expect(result.synthesizedInvariant!.formalExpression).toContain("(check-sat)");
    expect(result.outcome.kind).toBe("non_codifiable");
  }, 30_000);
});
