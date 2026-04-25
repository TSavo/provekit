/**
 * Pitch-leak 3 layer 1: per-bug-class multi-shape principle generation.
 *
 * Verifies that C6 (tryExistingCapabilities + the generatePrincipleCandidate
 * stage) returns N >= 2 PrincipleCandidates when the LLM proposes multiple
 * alternative AST shapes for the same bug class. All returned shapes share
 * `bugClassId` (the cross-shape group key) but have distinct `name`.
 *
 * Stub-LLM only — no real LLM calls. The stub returns a "principles" array
 * response keyed on the C6 prompt's "static-analysis rule author" substring,
 * and adversarial-validation fixtures keyed on "security-minded adversary".
 *
 * Bug class chosen: division-by-zero (rich existing fixtures, two natural
 * alternative shapes: division and modulo, both unguarded operations).
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  unregisterCapability,
  listCapabilities,
} from "../sast/capabilityRegistry.js";
import { listRelations, registerRelation, _clearRelationRegistry } from "../dsl/relationRegistry.js";

// Side-effect imports: register all production capabilities (including
// `arithmetic` and its extractor) and built-in DSL relations. Without these,
// fixtures built via buildSASTForFile would have empty capability tables and
// the principle would never match the false-negative fixtures.
import "../sast/schema/capabilities/index.js";
import "../dsl/relations.js";
import type { BugSignal, InvariantClaim, FixCandidate, PrincipleCandidate } from "./types.js";
import { tryExistingCapabilities } from "./principleGen.js";
import { generatePrincipleCandidate } from "./stages/generatePrincipleCandidate.js";

// ---------------------------------------------------------------------------
// Registry snapshot/restore helpers
// ---------------------------------------------------------------------------

let savedCapabilities: ReturnType<typeof listCapabilities>;
let savedRelations: ReturnType<typeof listRelations>;

function snapshotRegistry(): void {
  savedCapabilities = [...listCapabilities()];
  savedRelations = [...listRelations()];
}

function restoreRegistry(): void {
  for (const cap of listCapabilities()) {
    if (!savedCapabilities.find((s) => s.dslName === cap.dslName)) {
      unregisterCapability(cap.dslName);
    }
  }
  const currentRels = listRelations();
  for (const rel of currentRels) {
    if (!savedRelations.find((s) => s.name === rel.name)) {
      _clearRelationRegistry();
      for (const saved of savedRelations) registerRelation(saved);
      break;
    }
  }
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

function makeSignal(overrides: Partial<BugSignal> = {}): BugSignal {
  return {
    source: "test",
    rawText: "denominator may be zero in arithmetic operation",
    summary: "Division by zero",
    failureDescription: "denominator can be zero",
    codeReferences: [],
    bugClassHint: "division-by-zero",
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
      description: "Add guard against zero denominator",
      fileEdits: [{ file: "src/math.ts", newContent: "if (d !== 0) return a / d;" }],
    },
    llmRationale: "Guard prevents zero divisor",
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

/** Minimal stub DB: returns empty rows from select(). Used for latentSiteMatches. */
function makeDb(): any {
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

/**
 * The `arithmetic` capability is auto-registered via the side-effect import
 * above. Tests just snapshot/restore around the registry to avoid leaking
 * state to other test files.
 */

// ---------------------------------------------------------------------------
// Adversarial validation fixtures (must satisfy oracle #6 thresholds).
// Each shape's principle is run against false-positives (must NOT match) +
// false-negatives (MUST match). For division-shape: positives use addition;
// negatives use division. For modulo-shape: positives use addition;
// negatives use modulo.
// ---------------------------------------------------------------------------

const ADVERSARIAL_DIV = JSON.stringify({
  false_positives: [
    { source: "function ok1(a: number, b: number) { return a + b; }" },
    { source: "function ok2(a: number, b: number) { return a - b; }" },
    { source: "function ok3(a: number, b: number) { return a * b; }" },
  ],
  false_negatives: [
    { source: "function bad1(a: number, b: number) { return a / b; }" },
    { source: "function bad2(a: number, b: number) { return (a + 1) / b; }" },
    { source: "function bad3(a: number, b: number) { return a / (b + 1); }" },
  ],
});

const ADVERSARIAL_MOD = JSON.stringify({
  false_positives: [
    { source: "function ok1(a: number, b: number) { return a + b; }" },
    { source: "function ok2(a: number, b: number) { return a - b; }" },
    { source: "function ok3(a: number, b: number) { return a * b; }" },
  ],
  false_negatives: [
    { source: "function bad1(a: number, b: number) { return a % b; }" },
    { source: "function bad2(a: number, b: number) { return (a + 1) % b; }" },
    { source: "function bad3(a: number, b: number) { return a % (b + 1); }" },
  ],
});

// ---------------------------------------------------------------------------
// Multi-shape proposal: canonical division + alternate modulo.
// Both shapes use only the registered `arithmetic` capability and no
// require/predicate clauses, so each compiles standalone and matches its
// respective negative fixtures.
// ---------------------------------------------------------------------------

const CANONICAL_DIV_DSL = `principle DivisionByZero {
  match $div: node where arithmetic.op == "/"
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}`;

const ALT_MOD_DSL = `principle DivisionByZeroModulo {
  match $mod: node where arithmetic.op == "%"
  report violation {
    at $mod
    captures { modulo: $mod }
    message "modulo divisor may be zero"
  }
}`;

const MULTI_SHAPE_RESPONSE = JSON.stringify({
  kind: "principles",
  bugClassId: "division-by-zero",
  principles: [
    {
      name: "DivisionByZero",
      dslSource: CANONICAL_DIV_DSL,
      smtTemplate: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
      teachingExample: {
        domain: "arithmetic",
        explanation: "Division denominator may be zero",
        smt2: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
      },
    },
    {
      name: "DivisionByZeroModulo",
      dslSource: ALT_MOD_DSL,
      smtTemplate: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
      teachingExample: {
        domain: "arithmetic",
        explanation: "Modulo divisor may be zero",
        smt2: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
      },
    },
  ],
});

/**
 * Build a stub LLM with prompt-aware dispatch.
 *
 * Cannot use plain StubLLMProvider Map matching here: both the C6 prompt and
 * the adversarial-fixture prompts contain `arithmetic.op == "/"` (the C6
 * prompt embeds division-by-zero.dsl as the exemplar). We need to dispatch
 * on a discriminator that's unique to each prompt:
 *   - C6 prompt contains "static-analysis rule author" (and NOT
 *     "security-minded adversary").
 *   - Adversarial prompt contains "security-minded adversary".
 *
 * Within the adversarial branch we route by the DSL operator the embedded
 * principle uses (DivisionByZeroModulo references "%", DivisionByZero "/").
 */
function buildPromptAwareLLM(c6Response: string): { complete: (params: { prompt: string; model?: string }) => Promise<string> } {
  return {
    complete: async ({ prompt }) => {
      // Adversarial prompt: embed contains principle DSL.
      if (prompt.includes("security-minded adversary")) {
        if (prompt.includes("DivisionByZeroModulo")) return ADVERSARIAL_MOD;
        if (prompt.includes("DivisionByZero")) return ADVERSARIAL_DIV;
        // Default to division fixtures.
        return ADVERSARIAL_DIV;
      }
      // C6 principle proposal.
      if (prompt.includes("static-analysis rule author")) return c6Response;
      throw new Error(`stub LLM: no canned response for prompt`);
    },
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("C6 multi-shape principle generation (pitch-leak 3 layer 1)", () => {
  beforeEach(snapshotRegistry);
  afterEach(restoreRegistry);

  it("tryExistingCapabilities returns >= 2 principles when LLM proposes alternative shapes", { timeout: 120_000 }, async () => {
    const llm = buildPromptAwareLLM(MULTI_SHAPE_RESPONSE);
    const result = await tryExistingCapabilities({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });

    expect(result.kind).toBe("ok");
    if (result.kind !== "ok") return; // Type narrowing.

    expect(result.principles.length).toBeGreaterThanOrEqual(2);

    // All shapes share the same bugClassId.
    const bugClassIds = new Set(result.principles.map((p) => p.bugClassId));
    expect(bugClassIds.size).toBe(1);
    expect(bugClassIds.has("division-by-zero")).toBe(true);

    // Shapes have distinct names.
    const names = new Set(result.principles.map((p) => p.name));
    expect(names.size).toBe(result.principles.length);

    // Canonical (index 0) is the canonical division-by-zero shape.
    expect(result.principles[0].name).toBe("DivisionByZero");
    expect(result.principles[0].dslSource).toContain(`arithmetic.op == "/"`);

    // Alternate shape catches modulo.
    expect(result.principles[1].name).toBe("DivisionByZeroModulo");
    expect(result.principles[1].dslSource).toContain(`arithmetic.op == "%"`);
  });

  it("generatePrincipleCandidate stage forwards multi-shape principles intact", { timeout: 120_000 }, async () => {
    const llm = buildPromptAwareLLM(MULTI_SHAPE_RESPONSE);
    const result: PrincipleCandidate[] = await generatePrincipleCandidate({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });

    expect(result.length).toBeGreaterThanOrEqual(2);

    // All entries share bugClassId.
    for (const p of result) {
      expect(p.bugClassId).toBe("division-by-zero");
    }

    // Names are unique across shapes.
    const names = new Set(result.map((p) => p.name));
    expect(names.size).toBe(result.length);
  });

  it("legacy single-principle response (kind: 'principle') is wrapped into length-1 array with bugClassId", { timeout: 60_000 }, async () => {
    const legacySingle = JSON.stringify({
      kind: "principle",
      name: "DivisionByZero",
      dslSource: CANONICAL_DIV_DSL,
      smtTemplate: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
      teachingExample: {
        domain: "arithmetic",
        explanation: "Division denominator may be zero",
        smt2: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
      },
    });

    const llm = buildPromptAwareLLM(legacySingle);

    const result = await tryExistingCapabilities({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });

    expect(result.kind).toBe("ok");
    if (result.kind !== "ok") return;

    expect(result.principles).toHaveLength(1);
    // bugClassId is derived from bugClassHint when not provided in legacy form.
    expect(result.principles[0].bugClassId).toBe("division-by-zero");
    expect(result.principles[0].name).toBe("DivisionByZero");
  });
});
