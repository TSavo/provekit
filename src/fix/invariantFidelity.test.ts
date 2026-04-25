/**
 * Oracle #1.5 — invariant fidelity check tests.
 *
 * Tests:
 *  - crossLlmAgreement: equivalent invariants → pass; conflicting → fail
 *  - traceabilityCheck: grounded citations → pass; ungrounded → fail
 *  - adversarialFixturePreValidation: correct classification → pass; mixed → fail
 *  - runInvariantFidelity: all pass → passed:true; any fail → passed:false + failures
 *  - formulateInvariant integration: fidelity fail → retry; retry pass → returns claim;
 *    both fail → throws InvariantFormulationFailed
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "../sast/builder.js";
import {
  crossLlmAgreement,
  traceabilityCheck,
  adversarialFixturePreValidation,
  runInvariantFidelity,
} from "./invariantFidelity.js";
import { formulateInvariant } from "./stages/formulateInvariant.js";
import { InvariantFormulationFailed } from "./types.js";
import type { InvariantClaim, BugSignal, LLMProvider } from "./types.js";
import type { FidelityCheckResult, FidelityVerifiers } from "./invariantFidelity.js";

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

const DIVIDE_BUG_SIGNAL: BugSignal = {
  source: "test",
  rawText: "calling divide(x, 0) produces Infinity because the denominator is zero",
  summary: "divide(a, b) crashes when b is zero",
  failureDescription: "Repro: compute(5, 0) — ZeroDivisionError when b equals zero",
  codeReferences: [{ file: "divide.ts", line: 1 }],
  bugClassHint: "division-by-zero",
};

/** A valid SAT-producing InvariantClaim for division-by-zero */
const VALID_CLAIM: InvariantClaim = {
  principleId: null,
  description: "b must not be zero before division",
  formalExpression: [
    "(declare-const a Int)",
    "(declare-const b Int)",
    "(assert (= b 0))",
    "(check-sat)",
  ].join("\n"),
  bindings: [
    { smt_constant: "a", source_expr: "a", source_line: 1, sort: "Int" },
    { smt_constant: "b", source_expr: "b", source_line: 1, sort: "Int" },
  ],
  complexity: 1,
  witness: "sat",
  citations: [
    { smt_clause: "(= b 0)", source_quote: "calling divide(x, 0) produces Infinity because the denominator is zero" },
  ],
};

// Adversary responses — must match the actual prompt sent by crossLlmAgreement.
// The adversary prompt contains "INDEPENDENTLY" as a distinctive substring.

/** Equivalent claim in same variable namespace */
const ADVERSARY_EQUIVALENT_RESPONSE = JSON.stringify({
  description: "denominator must not equal zero",
  smt_declarations: ["(declare-const a Int)", "(declare-const b Int)"],
  smt_violation_assertion: "(assert (= b 0))",
  bindings: [
    { smt_constant: "a", source_expr: "a", sort: "Int" },
    { smt_constant: "b", source_expr: "b", sort: "Int" },
  ],
});

/** Conflicting claim (extra constraint that proposer doesn't have) */
const ADVERSARY_CONFLICTING_RESPONSE = JSON.stringify({
  description: "a must be positive AND b must be negative",
  smt_declarations: ["(declare-const a Int)", "(declare-const b Int)"],
  smt_violation_assertion: "(assert (and (> a 0) (< b 0)))",
  bindings: [
    { smt_constant: "a", source_expr: "a", sort: "Int" },
    { smt_constant: "b", source_expr: "b", sort: "Int" },
  ],
});

// Traceability verifier responses — must match prompt substring.
// The verifier prompt contains "each citation, determine whether" as distinctive text.

/** Traceability verifier response: all grounded */
const TRACEABILITY_PASS_RESPONSE = JSON.stringify({ all_grounded: true });

/** Traceability verifier response: one ungrounded clause */
const TRACEABILITY_FAIL_RESPONSE = JSON.stringify({
  all_grounded: false,
  ungrounded: [{ smt_clause: "(= b 0)", reason: "quote not found in bug report" }],
});

// Fixture generation responses — prompt contains "Generate 5 POSITIVE fixtures".

/** Fixture response that classifies correctly (b=0 is SAT for positive, b=1 is UNSAT for negative) */
const FIXTURE_PASS_RESPONSE = JSON.stringify({
  positive: [
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 5, b: 0 }, description: "b is zero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 1, b: 0 }, description: "b is zero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 10, b: 0 }, description: "b is zero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 2, b: 0 }, description: "b is zero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 3, b: 0 }, description: "b is zero" },
  ],
  negative: [
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 5, b: 1 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 1, b: 2 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 10, b: 3 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 2, b: 4 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 3, b: 5 }, description: "b is nonzero" },
  ],
});

/** Fixture response: positive misclassified (b=1 → UNSAT for "(= b 0)", not positive) */
const FIXTURE_FAIL_RESPONSE = JSON.stringify({
  positive: [
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 5, b: 1 }, description: "wrong: b nonzero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 1, b: 2 }, description: "wrong: b nonzero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 10, b: 3 }, description: "wrong: b nonzero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 2, b: 4 }, description: "wrong: b nonzero" },
    { source: "function divide(a, b) { return a / b; }", inputBindings: { a: 3, b: 5 }, description: "wrong: b nonzero" },
  ],
  negative: [
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 5, b: 1 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 1, b: 2 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 10, b: 3 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 2, b: 4 }, description: "b is nonzero" },
    { source: "function divide(a, b) { if (!b) throw new Error(); return a / b; }", inputBindings: { a: 3, b: 5 }, description: "b is nonzero" },
  ],
});

// ---------------------------------------------------------------------------
// Helper: stub LLM (matches on prompt substring)
// ---------------------------------------------------------------------------

function makeStubLlm(responses: Map<string, string>): LLMProvider {
  return {
    async complete(params) {
      for (const [key, value] of responses) {
        if (params.prompt.includes(key)) return value;
      }
      throw new Error(`stub LLM: no response for prompt. Keys: ${[...responses.keys()].join(", ")}\nPrompt snippet: ${params.prompt.slice(0, 200)}`);
    },
  };
}

// ---------------------------------------------------------------------------
// 1. crossLlmAgreement
// ---------------------------------------------------------------------------

describe("crossLlmAgreement", () => {
  // The adversary prompt contains "INDEPENDENTLY" as a distinctive substring.

  it("passes when adversary derives semantically equivalent invariant", async () => {
    const llm = makeStubLlm(new Map([["INDEPENDENTLY", ADVERSARY_EQUIVALENT_RESPONSE]]));
    const result = await crossLlmAgreement({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/PASS/);
  });

  it("fails when adversary derives a conflicting invariant", async () => {
    const llm = makeStubLlm(new Map([["INDEPENDENTLY", ADVERSARY_CONFLICTING_RESPONSE]]));
    const result = await crossLlmAgreement({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/FAIL/);
  });

  it("fails when adversary LLM call throws", async () => {
    const llm: LLMProvider = {
      complete: async () => { throw new Error("timeout"); },
    };
    const result = await crossLlmAgreement({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toContain("timeout");
  });

  it("fails when adversary returns unparseable JSON", async () => {
    const llm = makeStubLlm(new Map([["INDEPENDENTLY", "this is not json at all"]]));
    const result = await crossLlmAgreement({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/parse/i);
  });

  it("degrades to text-similarity when binding sets differ (adversary has extra constant)", async () => {
    // Adversary uses source_expr "result" which proposer doesn't have → cannot alpha-rename
    const adversaryDifferentBindings = JSON.stringify({
      description: "b must not be zero",
      smt_declarations: ["(declare-const b Int)", "(declare-const result Int)"],
      smt_violation_assertion: "(assert (= b 0))",
      bindings: [
        { smt_constant: "b", source_expr: "b", sort: "Int" },
        { smt_constant: "result", source_expr: "result", sort: "Int" },
      ],
    });
    const llm = makeStubLlm(new Map([["INDEPENDENTLY", adversaryDifferentBindings]]));
    const result = await crossLlmAgreement({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    // Descriptions both mention "zero" → text-similarity ratio is high enough → pass
    expect(result.detail).toMatch(/DEGRADED/);
    expect(result.passed).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// 2. traceabilityCheck
// ---------------------------------------------------------------------------

describe("traceabilityCheck", () => {
  // The verifier prompt contains "each citation, determine whether" as distinctive text.

  it("passes when all citations are grounded in the bug report", async () => {
    const llm = makeStubLlm(new Map([["each citation, determine whether", TRACEABILITY_PASS_RESPONSE]]));
    const result = await traceabilityCheck({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/PASS/);
  });

  it("fails when verifier reports ungrounded citations", async () => {
    const llm = makeStubLlm(new Map([["each citation, determine whether", TRACEABILITY_FAIL_RESPONSE]]));
    const result = await traceabilityCheck({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/FAIL/);
    expect(result.detail).toContain("(= b 0)");
  });

  it("fails when invariant has no citations", async () => {
    const claimNoCitations: InvariantClaim = { ...VALID_CLAIM, citations: null };
    const llm: LLMProvider = { complete: async () => { throw new Error("should not be called"); } };
    const result = await traceabilityCheck({ invariant: claimNoCitations, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/no citations/i);
  });

  it("fails when citations array is empty", async () => {
    const claimEmptyCitations: InvariantClaim = { ...VALID_CLAIM, citations: [] };
    const llm: LLMProvider = { complete: async () => { throw new Error("should not be called"); } };
    const result = await traceabilityCheck({ invariant: claimEmptyCitations, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/no citations/i);
  });

  it("fails when verifier LLM throws", async () => {
    const llm: LLMProvider = { complete: async () => { throw new Error("network error"); } };
    const result = await traceabilityCheck({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toContain("network error");
  });
});

// ---------------------------------------------------------------------------
// 3. adversarialFixturePreValidation
// ---------------------------------------------------------------------------

describe("adversarialFixturePreValidation", () => {
  // The fixture prompt contains "Generate 5 POSITIVE fixtures" as distinctive text.

  it("passes when all positive fixtures classify SAT and all negative classify UNSAT", async () => {
    const llm = makeStubLlm(new Map([["Generate 5 POSITIVE fixtures", FIXTURE_PASS_RESPONSE]]));
    const result = await adversarialFixturePreValidation({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/PASS/);
    expect(result.detail).toMatch(/positive=5\/5/);
    expect(result.detail).toMatch(/negative=5\/5/);
  });

  it("fails when positive fixtures are misclassified (b=1 is UNSAT for '= b 0')", async () => {
    const llm = makeStubLlm(new Map([["Generate 5 POSITIVE fixtures", FIXTURE_FAIL_RESPONSE]]));
    const result = await adversarialFixturePreValidation({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/FAIL/);
    expect(result.detail).toMatch(/positive/);
  });

  it("fails when fixture LLM call throws", async () => {
    const llm: LLMProvider = { complete: async () => { throw new Error("quota exceeded"); } };
    const result = await adversarialFixturePreValidation({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toContain("quota exceeded");
  });

  it("fails when fixture response is malformed", async () => {
    const llm = makeStubLlm(new Map([["Generate 5 POSITIVE fixtures", "not json"]]));
    const result = await adversarialFixturePreValidation({ invariant: VALID_CLAIM, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/parse/i);
  });
});

// ---------------------------------------------------------------------------
// 4. runInvariantFidelity orchestration
// ---------------------------------------------------------------------------

describe("runInvariantFidelity", () => {
  it("skips check and returns passed:true for principle-match invariants", async () => {
    const principleMatchClaim: InvariantClaim = { ...VALID_CLAIM, principleId: "division-by-zero" };
    const llm: LLMProvider = { complete: async () => { throw new Error("should not be called"); } };
    const result = await runInvariantFidelity({ invariant: principleMatchClaim, signal: DIVIDE_BUG_SIGNAL, llm });
    expect(result.passed).toBe(true);
    expect(result.failures).toHaveLength(0);
  });

  it("returns passed:true when all three verifiers pass", async () => {
    const verifiers: FidelityVerifiers = {
      crossLlmAgreement: async () => ({ passed: true, detail: "agreement" }),
      traceabilityCheck: async () => ({ passed: true, detail: "grounded" }),
      adversarialFixturePreValidation: async () => ({ passed: true, detail: "fixtures ok" }),
    };
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };
    const result = await runInvariantFidelity({
      invariant: VALID_CLAIM,
      signal: DIVIDE_BUG_SIGNAL,
      llm,
      _verifiers: verifiers,
    });
    expect(result.passed).toBe(true);
    expect(result.failures).toHaveLength(0);
  });

  it("returns passed:false with all three failure details when all verifiers fail", async () => {
    const verifiers: FidelityVerifiers = {
      crossLlmAgreement: async () => ({ passed: false, detail: "disagreement reason" }),
      traceabilityCheck: async () => ({ passed: false, detail: "ungrounded clause" }),
      adversarialFixturePreValidation: async () => ({ passed: false, detail: "fixture mismatch" }),
    };
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };
    const result = await runInvariantFidelity({
      invariant: VALID_CLAIM,
      signal: DIVIDE_BUG_SIGNAL,
      llm,
      _verifiers: verifiers,
    });
    expect(result.passed).toBe(false);
    expect(result.failures).toHaveLength(3);
    expect(result.failures.some((f) => f.includes("disagreement reason"))).toBe(true);
    expect(result.failures.some((f) => f.includes("ungrounded clause"))).toBe(true);
    expect(result.failures.some((f) => f.includes("fixture mismatch"))).toBe(true);
  });

  it("returns passed:false with just the failing verifier detail when one fails", async () => {
    const verifiers: FidelityVerifiers = {
      crossLlmAgreement: async () => ({ passed: true, detail: "ok" }),
      traceabilityCheck: async () => ({ passed: false, detail: "ungrounded: (= b 0)" }),
      adversarialFixturePreValidation: async () => ({ passed: true, detail: "ok" }),
    };
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };
    const result = await runInvariantFidelity({
      invariant: VALID_CLAIM,
      signal: DIVIDE_BUG_SIGNAL,
      llm,
      _verifiers: verifiers,
    });
    expect(result.passed).toBe(false);
    expect(result.failures).toHaveLength(1);
    expect(result.failures[0]).toContain("(= b 0)");
  });

  it("runs all three verifiers even when the first one fails (no short-circuit)", async () => {
    const callLog: string[] = [];
    const verifiers: FidelityVerifiers = {
      crossLlmAgreement: async () => { callLog.push("cross"); return { passed: false, detail: "fail" }; },
      traceabilityCheck: async () => { callLog.push("trace"); return { passed: true, detail: "ok" }; },
      adversarialFixturePreValidation: async () => { callLog.push("fixture"); return { passed: true, detail: "ok" }; },
    };
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };
    await runInvariantFidelity({
      invariant: VALID_CLAIM,
      signal: DIVIDE_BUG_SIGNAL,
      llm,
      _verifiers: verifiers,
    });
    // All three must have been called (Promise.all, not short-circuit)
    expect(callLog).toContain("cross");
    expect(callLog).toContain("trace");
    expect(callLog).toContain("fixture");
  });
});

// ---------------------------------------------------------------------------
// 5. formulateInvariant integration: oracle #1.5 wired in
// ---------------------------------------------------------------------------

describe("formulateInvariant integration: oracle #1.5", () => {
  let tmpDir: string;
  let db: ReturnType<typeof openDb>;

  afterEach(() => {
    try { db.$client.close(); } catch { /* ignore */ }
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  function openTestDb() {
    tmpDir = mkdtempSync(join(tmpdir(), "provekit-fidelity-test-"));
    const dbPath = join(tmpDir, "test.db");
    const testDb = openDb(dbPath);
    migrate(testDb, { migrationsFolder: "./drizzle" });
    db = testDb;
    return db;
  }

  function writeFixtureFile(name: string, content: string): string {
    const p = join(tmpDir, name);
    writeFileSync(p, content, "utf8");
    return p;
  }

  const VALID_LLM_RESPONSE = JSON.stringify({
    description: "b must not be zero before division",
    smt_declarations: ["(declare-const a Int)", "(declare-const b Int)"],
    smt_violation_assertion: "(assert (= b 0))",
    bindings: [
      { smt_constant: "a", source_expr: "a", sort: "Int" },
      { smt_constant: "b", source_expr: "b", sort: "Int" },
    ],
    citations: [
      { smt_clause: "(= b 0)", source_quote: "calling divide(x, 0) produces Infinity" },
    ],
  });

  function makeTestSignal(filePath: string): BugSignal {
    return {
      source: "test",
      rawText: "calling divide(x, 0) produces Infinity",
      summary: "division by zero",
      failureDescription: "b can be zero",
      codeReferences: [{ file: filePath, line: 1 }],
    };
  }

  function makeTestLocus(filePath: string) {
    return {
      file: filePath,
      line: 1,
      confidence: 1.0,
      // Use a fake node ID that will NOT be in principle_matches → forces novel LLM path
      primaryNode: "fake-novel-node-not-in-principle-matches",
      containingFunction: "fake-novel-node-not-in-principle-matches",
      relatedFunctions: [] as string[],
      dataFlowAncestors: [] as string[],
      dataFlowDescendants: [] as string[],
      dominanceRegion: [] as string[],
      postDominanceRegion: [] as string[],
    };
  }

  it("returns invariant when fidelity passes on first attempt", async () => {
    const testDb = openTestDb();
    const src = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixtureFile("div.ts", src);
    buildSASTForFile(testDb, filePath);

    // "You are a formal verification expert" is a distinctive phrase in buildLlmPrompt
    const llm = makeStubLlm(new Map([["formal verification expert", VALID_LLM_RESPONSE]]));

    const allPass: FidelityVerifiers = {
      crossLlmAgreement: async () => ({ passed: true, detail: "agreement" }),
      traceabilityCheck: async () => ({ passed: true, detail: "grounded" }),
      adversarialFixturePreValidation: async () => ({ passed: true, detail: "fixtures ok" }),
    };

    const claim = await formulateInvariant({
      signal: makeTestSignal(filePath),
      locus: makeTestLocus(filePath),
      db: testDb,
      llm,
      _fidelityVerifiers: allPass,
    });

    expect(claim.principleId).toBeNull();
    expect(claim.description).toContain("zero");
    expect(claim.formalExpression).toContain("(check-sat)");
    expect(claim.citations).toBeDefined();
    expect(claim.citations!.length).toBeGreaterThan(0);
  });

  it("retries and returns when first fidelity fails but retry passes", async () => {
    const testDb = openTestDb();
    const src = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixtureFile("div2.ts", src);
    buildSASTForFile(testDb, filePath);

    // Both prompt variants (initial + retry) produce valid SMT responses
    const llm = makeStubLlm(new Map([["formal verification expert", VALID_LLM_RESPONSE]]));

    let fidelityCallCount = 0;
    const failThenPass: FidelityVerifiers = {
      crossLlmAgreement: async () => {
        fidelityCallCount++;
        if (fidelityCallCount === 1) return { passed: false, detail: "first attempt disagreement" };
        return { passed: true, detail: "retry passed" };
      },
      traceabilityCheck: async () => ({ passed: true, detail: "grounded" }),
      adversarialFixturePreValidation: async () => ({ passed: true, detail: "fixtures ok" }),
    };

    const claim = await formulateInvariant({
      signal: makeTestSignal(filePath),
      locus: makeTestLocus(filePath),
      db: testDb,
      llm,
      _fidelityVerifiers: failThenPass,
    });

    expect(claim.principleId).toBeNull();
    // crossLlmAgreement called once per fidelity run, two fidelity runs total
    expect(fidelityCallCount).toBe(2);
  });

  it("throws InvariantFormulationFailed when fidelity fails on both attempts", async () => {
    const testDb = openTestDb();
    const src = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixtureFile("div3.ts", src);
    buildSASTForFile(testDb, filePath);

    const llm = makeStubLlm(new Map([["formal verification expert", VALID_LLM_RESPONSE]]));

    const alwaysFail: FidelityVerifiers = {
      crossLlmAgreement: async () => ({ passed: false, detail: "disagreement always" }),
      traceabilityCheck: async () => ({ passed: false, detail: "ungrounded always" }),
      adversarialFixturePreValidation: async () => ({ passed: false, detail: "fixture fail always" }),
    };

    await expect(
      formulateInvariant({
        signal: makeTestSignal(filePath),
        locus: makeTestLocus(filePath),
        db: testDb,
        llm,
        _fidelityVerifiers: alwaysFail,
      }),
    ).rejects.toThrow(InvariantFormulationFailed);

    await expect(
      formulateInvariant({
        signal: makeTestSignal(filePath),
        locus: makeTestLocus(filePath),
        db: testDb,
        llm,
        _fidelityVerifiers: alwaysFail,
      }),
    ).rejects.toThrow(/fidelity/i);
  });
});
