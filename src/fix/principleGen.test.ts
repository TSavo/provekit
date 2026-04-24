/**
 * C6: principleGen.ts tests
 *
 * Tests tryExistingCapabilities, proposeWithCapability, and runAdversarialValidation.
 * All tests use StubLLMProvider — no real LLM calls.
 *
 * Registry interaction: snapshot + restore via beforeEach/afterEach to avoid
 * polluting other tests that depend on registered capabilities.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  listCapabilities,
  registerCapability,
  unregisterCapability,
  _clearRegistry,
} from "../sast/capabilityRegistry.js";
import { StubLLMProvider } from "./types.js";
import type { BugSignal, InvariantClaim, FixCandidate } from "./types.js";
import { tryExistingCapabilities, runAdversarialValidation } from "./principleGen.js";
import { generatePrincipleCandidate } from "./stages/generatePrincipleCandidate.js";

// ---------------------------------------------------------------------------
// Registry helpers — snapshot + restore
// ---------------------------------------------------------------------------

let savedCapabilities: ReturnType<typeof listCapabilities>;

function snapshotRegistry(): void {
  savedCapabilities = [...listCapabilities()];
}

function restoreRegistry(): void {
  // Remove anything added since snapshot.
  const current = listCapabilities();
  for (const cap of current) {
    if (!savedCapabilities.find((s) => s.dslName === cap.dslName)) {
      unregisterCapability(cap.dslName);
    }
  }
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

function makeSignal(overrides: Partial<BugSignal> = {}): BugSignal {
  return {
    source: "test",
    rawText: "divide by zero at line 3",
    summary: "Division by zero",
    failureDescription: "denominator can be zero",
    codeReferences: [],
    bugClassHint: "arithmetic",
    ...overrides,
  };
}

function makeInvariant(overrides: Partial<InvariantClaim> = {}): InvariantClaim {
  return {
    principleId: null,
    description: "denominator must not be zero",
    formalExpression: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
    bindings: [],
    complexity: 1,
    witness: "d = 0",
    ...overrides,
  };
}

function makeFixCandidate(overrides: Partial<FixCandidate> = {}): FixCandidate {
  return {
    patch: {
      description: "Add guard for zero denominator",
      fileEdits: [{ file: "src/math.ts", newContent: "if (d !== 0) return a / d;" }],
    },
    llmRationale: "Guards against zero",
    llmConfidence: 0.9,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 1,
      overlayClosed: false,
    },
    ...overrides,
  };
}

function makeDb(): any {
  // Minimal stub DB — C6 only uses it for latentSiteMatches queries which
  // will return empty when the DB has no tables. We use a plain object that
  // returns empty arrays from select().
  return {
    select: () => ({
      from: () => ({
        where: () => ({ get: () => null, all: () => [] }),
        get: () => null,
        all: () => [],
      }),
    }),
    $client: { prepare: () => ({ all: () => [] }) },
  };
}

// ---------------------------------------------------------------------------
// Tests: generatePrincipleCandidate (the stage entry point)
// ---------------------------------------------------------------------------

describe("generatePrincipleCandidate", () => {
  beforeEach(snapshotRegistry);
  afterEach(restoreRegistry);

  it("returns null when invariant has a principleId (already covered)", async () => {
    const llm = new StubLLMProvider(new Map());
    const invariant = makeInvariant({ principleId: "existing-principle" });
    const result = await generatePrincipleCandidate({
      signal: makeSignal(),
      invariant,
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });
    expect(result).toBeNull();
  });

  it("returns null when LLM returns malformed response", async () => {
    const llm = new StubLLMProvider(
      new Map([["denominator", '{"kind": "INVALID_KIND"}']]),
    );
    const result = await generatePrincipleCandidate({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });
    expect(result).toBeNull();
  });

  it("returns null when LLM explicitly reports non_codifiable", async () => {
    const llm = new StubLLMProvider(
      new Map([
        ["denominator", '{"kind": "non_codifiable", "reason": "too runtime-specific"}'],
      ]),
    );
    const result = await generatePrincipleCandidate({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });
    expect(result).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Tests: tryExistingCapabilities
// ---------------------------------------------------------------------------

describe("tryExistingCapabilities", () => {
  beforeEach(snapshotRegistry);
  afterEach(restoreRegistry);

  it("returns non_codifiable on LLM call failure (throws)", async () => {
    const llm = {
      complete: async () => { throw new Error("rate limit"); },
    };
    const result = await tryExistingCapabilities({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });
    expect(result.kind).toBe("non_codifiable");
  });

  it("returns capability_gap when LLM explicitly needs_capability", async () => {
    const llm = new StubLLMProvider(
      new Map([
        [
          "denominator",
          JSON.stringify({
            kind: "needs_capability",
            missing_predicate: "tracks null-safe property access",
          }),
        ],
      ]),
    );
    const result = await tryExistingCapabilities({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });
    expect(result.kind).toBe("capability_gap");
    if (result.kind === "capability_gap") {
      expect(result.gap).toContain("null-safe");
    }
  });

  it("routes to capability_gap when DSL references unknown capability", async () => {
    // DSL that uses a capability not registered (correct DSL syntax).
    const dslWithUnknownCap = `principle TestP {
  match $x: node where unknownCapability.column == "val"
  report violation {
    at $x
    captures { site: $x }
    message "bad"
  }
}`;
    const llm = new StubLLMProvider(
      new Map([
        [
          "denominator",
          JSON.stringify({
            kind: "principle",
            name: "TestP",
            dslSource: dslWithUnknownCap,
            smtTemplate: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
            teachingExample: {
              domain: "arithmetic",
              explanation: "test",
              smt2: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
            },
          }),
        ],
      ]),
    );
    const result = await tryExistingCapabilities({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });
    expect(result.kind).toBe("capability_gap");
  });
});

// ---------------------------------------------------------------------------
// Tests: runAdversarialValidation (oracle #6)
// ---------------------------------------------------------------------------

describe("runAdversarialValidation (oracle #6)", () => {
  beforeEach(snapshotRegistry);
  afterEach(restoreRegistry);

  it("fails when adversarial LLM returns malformed JSON", async () => {
    // Register a minimal capability so the DSL can compile.
    registerCapability({
      dslName: "arithmetic",
      table: { _: { name: "node_arithmetic" } } as any,
      columns: {
        node_id: {
          dslName: "node_id",
          drizzleColumn: { name: "node_id" },
          isNodeRef: true,
          nullable: false,
        },
        op: {
          dslName: "op",
          drizzleColumn: { name: "op" },
          isNodeRef: false,
          nullable: false,
          kindEnum: ["+", "-", "*", "/"],
        },
      },
    });

    const dslSource = `principle DivByZero {
  match $x: node where arithmetic.op == "/"
  report violation {
    at $x
    captures { site: $x }
    message "division"
  }
}`;

    const llm = new StubLLMProvider(
      new Map([["false-positive", "NOT VALID JSON {{{"]]),
    );
    const result = await runAdversarialValidation(
      dslSource,
      "denominator must not be zero",
      llm,
      makeDb(),
    );
    expect(result.passed).toBe(false);
    expect(result.evidence).toContain("malformed");
  });

  it("uses a different model than the proposer model", async () => {
    // Track which model the adversarial call uses.
    const calls: string[] = [];
    const llm = {
      complete: async (params: { prompt: string; model?: string }) => {
        calls.push(params.model ?? "default");
        if (params.model === "haiku" || params.model === "sonnet" || !params.model) {
          return JSON.stringify({
            false_positives: [],
            false_negatives: [],
          });
        }
        return "{}";
      },
    };

    registerCapability({
      dslName: "arithmetic",
      table: { _: { name: "node_arithmetic" } } as any,
      columns: {
        node_id: {
          dslName: "node_id",
          drizzleColumn: { name: "node_id" },
          isNodeRef: true,
          nullable: false,
        },
        op: {
          dslName: "op",
          drizzleColumn: { name: "op" },
          isNodeRef: false,
          nullable: false,
        },
      },
    });

    const dslSource = `principle DivByZero {
  match $x: node where arithmetic.op == "/"
  report violation {
    at $x
    captures { site: $x }
    message "division"
  }
}`;

    await runAdversarialValidation(
      dslSource,
      "denominator must not be zero",
      llm,
      makeDb(),
      { proposerModel: "sonnet" },
    );

    // Adversarial should use "haiku" when proposer is "sonnet".
    expect(calls).toContain("haiku");
  });

  it("live registry is not mutated after adversarial validation", async () => {
    const before = listCapabilities().map((c) => c.dslName).sort();

    const llm = new StubLLMProvider(
      new Map([
        [
          "false-positive",
          JSON.stringify({ false_positives: [], false_negatives: [] }),
        ],
      ]),
    );

    await runAdversarialValidation(
      // DSL with correct syntax — will fail to compile if arithmetic not registered (fine for this test).
      `principle X {
  match $x: node where arithmetic.op == "/"
  report violation {
    at $x
    captures { site: $x }
    message "x"
  }
}`,
      "test",
      llm,
      makeDb(),
    );

    const after = listCapabilities().map((c) => c.dslName).sort();
    expect(after).toEqual(before);
  });
});
