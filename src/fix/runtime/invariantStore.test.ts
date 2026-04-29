/**
 * Tests for buildStoredInvariant's callsite + bindingLocations overrides.
 *
 * Covers issues #138 and #139: when the orchestrator's persistence path
 * derives geometry from C3's patch (rather than B-stage Locate), the
 * StoredInvariant's callsite.filePath, callsite.startLine, and each
 * binding's node.{filePath,startLine,endLine} reflect the post-edit
 * source — not the pre-edit locus.
 */

import { describe, it, expect } from "vitest";
import { buildStoredInvariant, type LocalBinding } from "./invariantStore.js";

// Tests build invariants via buildStoredInvariant which always emits
// LocalBindings in v1. Narrow at access sites so the union doesn't shed
// the .node accessor.
function asLocal(b: { type?: string }): LocalBinding {
  if (b.type !== "local") {
    throw new Error(`expected local binding, got type=${b.type}`);
  }
  return b as unknown as LocalBinding;
}
import type {
  InvariantClaim,
  BugSignal,
  BugLocus,
} from "../types.js";
import {
  pickPrimaryPatchEdit,
  findExpressionLines,
  findFunctionLine,
  offsetToLine,
} from "./patchUtils.js";
import type { FixCandidate } from "../types.js";

function makeSignal(): BugSignal {
  return {
    source: "test",
    rawText: "",
    summary: "ORDER BY direction is reversed in forRevision",
    failureDescription: "asc/desc swap",
    codeReferences: [],
  };
}

function makeLocus(): BugLocus {
  // Locate's BAD guess (low confidence) — unrelated file the verify CLI
  // can't resolve a substrate node for.
  return {
    file: "reference/toolstac/core/prompt-evolution-service.ts",
    line: 1,
    function: "forRevision",
    confidence: 0.3,
    primaryNode: "n0",
    containingFunction: "n0",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };
}

function makeClaim(): InvariantClaim {
  return {
    principleId: null,
    description: "order by must use desc",
    formalExpression: "(declare-const dir String)\n(assert (= dir \"asc\"))",
    bindings: [
      {
        smt_constant: "dir",
        source_line: 0,
        source_expr: "orderBy(asc(invocations.timestamp))",
        sort: "String",
      },
    ],
    complexity: 1,
    witness: null,
    citations: [],
    llmKind: "order",
  };
}

describe("buildStoredInvariant overrides (issues #138, #139)", () => {
  const signal = makeSignal();
  const locus = makeLocus();
  const claim = makeClaim();

  it("legacy path: with no overrides, callsite + bindings come from locus", () => {
    const stored = buildStoredInvariant({
      claim,
      signal,
      locus,
      test: null,
      patchSha: null,
      bindingNodeHashes: new Map(),
    });
    expect(stored.callsite.filePath).toBe(locus.file);
    expect(stored.callsite.startLine).toBe(locus.line);
    expect(asLocal(stored.bindings[0]).node.filePath).toBe(locus.file);
    // Legacy: pre-edit source_line guess (here 0).
    expect(asLocal(stored.bindings[0]).node.startLine).toBe(0);
  });

  it("override path: callsite filePath + startLine come from C3's patch", () => {
    const stored = buildStoredInvariant({
      claim,
      signal,
      locus,
      test: null,
      patchSha: null,
      bindingNodeHashes: new Map(),
      callsiteOverride: {
        filePath: "src/store/sqlite/repositories.ts",
        startLine: 117,
        endLine: 117,
      },
    });
    expect(stored.callsite.filePath).toBe("src/store/sqlite/repositories.ts");
    expect(stored.callsite.startLine).toBe(117);
    expect(stored.callsite.endLine).toBe(117);
  });

  it("override path: bindings[].node fields come from bindingLocations map", () => {
    const stored = buildStoredInvariant({
      claim,
      signal,
      locus,
      test: null,
      patchSha: null,
      bindingNodeHashes: new Map(),
      bindingLocations: new Map([
        [
          "dir",
          {
            filePath: "src/store/sqlite/repositories.ts",
            startLine: 121,
            endLine: 121,
          },
        ],
      ]),
    });
    expect(asLocal(stored.bindings[0]).node.filePath).toBe(
      "src/store/sqlite/repositories.ts",
    );
    expect(asLocal(stored.bindings[0]).node.startLine).toBe(121);
    expect(asLocal(stored.bindings[0]).node.endLine).toBe(121);
  });

  it("hash is independent of node.{filePath,startLine,endLine} (extra metadata)", () => {
    const a = buildStoredInvariant({
      claim,
      signal,
      locus,
      test: null,
      patchSha: null,
      bindingNodeHashes: new Map(),
    });
    const b = buildStoredInvariant({
      claim,
      signal,
      locus,
      test: null,
      patchSha: null,
      bindingNodeHashes: new Map(),
      callsiteOverride: {
        filePath: "totally/different.ts",
        startLine: 999,
        endLine: 999,
      },
      bindingLocations: new Map([
        [
          "dir",
          { filePath: "totally/different.ts", startLine: 50, endLine: 50 },
        ],
      ]),
    });
    expect(a.id).toBe(b.id);
  });

  it("bindings missing from bindingLocations fall back to locus shape", () => {
    const claim2: InvariantClaim = {
      ...claim,
      bindings: [
        ...claim.bindings,
        {
          smt_constant: "x",
          source_line: 42,
          source_expr: "x.foo",
          sort: "Int",
        },
      ],
    };
    const stored = buildStoredInvariant({
      claim: claim2,
      signal,
      locus,
      test: null,
      patchSha: null,
      bindingNodeHashes: new Map(),
      bindingLocations: new Map([
        [
          "dir",
          {
            filePath: "src/store/sqlite/repositories.ts",
            startLine: 121,
            endLine: 121,
          },
        ],
      ]),
    });
    // dir: from override
    const dir = stored.bindings.find((b) => b.smt_constant === "dir")!;
    expect(asLocal(dir).node.filePath).toBe("src/store/sqlite/repositories.ts");
    expect(asLocal(dir).node.startLine).toBe(121);
    // x: not in map → falls back to locus.file + b.source_line
    const x = stored.bindings.find((b) => b.smt_constant === "x")!;
    expect(asLocal(x).node.filePath).toBe(locus.file);
    expect(asLocal(x).node.startLine).toBe(42);
  });
});

describe("patchUtils helpers", () => {
  it("pickPrimaryPatchEdit returns the longest edit's content + path", () => {
    const fix = {
      patch: {
        fileEdits: [
          { file: "a.ts", newContent: "tiny" },
          {
            file: "b.ts",
            newContent: "this is the meatier file with more content",
          },
          { file: "c.ts", newContent: "medium length content here" },
        ],
        description: "test",
      },
      source: "llm",
      llmRationale: "",
      llmConfidence: 1,
      invariantHoldsUnderOverlay: true,
      overlayZ3Verdict: "unsat",
      audit: {
        overlayCreated: true,
        patchApplied: true,
        overlayReindexed: true,
        z3RunMs: 0,
        overlayClosed: false,
      },
    } satisfies FixCandidate;
    const primary = pickPrimaryPatchEdit(fix);
    expect(primary?.file).toBe("b.ts");
  });

  it("offsetToLine returns 1 for offset 0", () => {
    expect(offsetToLine("abc\ndef", 0)).toBe(1);
  });

  it("offsetToLine counts newlines correctly", () => {
    const text = "line1\nline2\nline3\nline4";
    // Position of "line3" starts at index 12 (5+1+5+1)
    expect(offsetToLine(text, 12)).toBe(3);
  });

  it("findExpressionLines finds the expression and returns 1-indexed line", () => {
    const text = "function forRevision() {\n  return db\n    .orderBy(asc(invocations.timestamp))\n    .limit(1);\n}";
    const found = findExpressionLines(text, "asc(invocations.timestamp)");
    expect(found).not.toBeNull();
    expect(found?.startLine).toBe(3);
    expect(found?.endLine).toBe(3);
  });

  it("findExpressionLines returns null when needle is absent", () => {
    expect(findExpressionLines("hello world", "missing")).toBeNull();
  });

  it("findExpressionLines handles multi-line spans (endLine > startLine)", () => {
    const text = "x = (\n  a\n  + b\n);";
    const found = findExpressionLines(text, "a\n  + b");
    expect(found?.startLine).toBe(2);
    expect(found?.endLine).toBe(3);
  });

  it("findFunctionLine matches `function NAME(`", () => {
    const text = "// header\n\nfunction forRevision(id: string) {\n  return id;\n}\n";
    expect(findFunctionLine(text, "forRevision")).toBe(3);
  });

  it("findFunctionLine matches `const NAME =` arrow assignment", () => {
    const text = "import x from 'y';\n\nconst forRevision = (id) => id;\n";
    expect(findFunctionLine(text, "forRevision")).toBe(3);
  });

  it("findFunctionLine returns null for missing function", () => {
    expect(findFunctionLine("nothing here", "forRevision")).toBeNull();
  });
});
