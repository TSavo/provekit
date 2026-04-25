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
  proseJaccardAgreement,
  classifyInvariantKind,
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
// 4b. classifyInvariantKind
// ---------------------------------------------------------------------------

describe("classifyInvariantKind", () => {
  it("classifies Int-binding arithmetic invariant as concrete", () => {
    expect(classifyInvariantKind(VALID_CLAIM)).toBe("concrete");
  });

  it("classifies Bool-only-binding invariant as abstract", () => {
    const boolClaim: InvariantClaim = {
      ...VALID_CLAIM,
      formalExpression: [
        "(declare-const tainted Bool)",
        "(declare-const sanitized Bool)",
        "(assert (and tainted (not sanitized)))",
        "(check-sat)",
      ].join("\n"),
      bindings: [
        { smt_constant: "tainted", source_expr: "input", source_line: 1, sort: "Bool" },
        { smt_constant: "sanitized", source_expr: "input", source_line: 1, sort: "Bool" },
      ],
    };
    expect(classifyInvariantKind(boolClaim)).toBe("abstract");
  });

  it("classifies invariant with no Int/Real declarations as abstract", () => {
    // Mixed bindings but no Int/Real in the SMT body
    const mixedClaim: InvariantClaim = {
      ...VALID_CLAIM,
      formalExpression: "(declare-const x Bool)\n(assert x)\n(check-sat)",
      bindings: [
        { smt_constant: "x", source_expr: "x", source_line: 1, sort: "Bool" },
      ],
    };
    expect(classifyInvariantKind(mixedClaim)).toBe("abstract");
  });

  it("classifies Bool-only SMT as abstract even when bindings claim Int (v4-dogfood regression)", () => {
    // formulateInvariant.ts:403 defaults missing sort fields to "Int"; the
    // proposer LLM may emit Bool-only SMT with no `sort` in bindings, leaving
    // the bindings array claiming Int while the actual SMT is Bool. The
    // classifier must trust the SMT body, not the bindings.
    const v4Regression: InvariantClaim = {
      ...VALID_CLAIM,
      formalExpression: [
        "(declare-const tainted Bool)",
        "(declare-const sanitized Bool)",
        "(assert (and tainted (not sanitized)))",
        "(check-sat)",
      ].join("\n"),
      bindings: [
        // Defaulted to Int by formulateInvariant parser when LLM omitted sort
        { smt_constant: "tainted", source_expr: "input", source_line: 3, sort: "Int" },
        { smt_constant: "sanitized", source_expr: "input", source_line: 3, sort: "Int" },
      ],
    };
    expect(classifyInvariantKind(v4Regression)).toBe("abstract");
  });

  it("requires actual (declare-const ... Int) decl, not just the word 'Int' in a comment", () => {
    const commentNoDecl: InvariantClaim = {
      ...VALID_CLAIM,
      formalExpression: [
        "; This invariant tracks Int taintedness (no actual Int decl)",
        "(declare-const x Bool)",
        "(assert x)",
        "(check-sat)",
      ].join("\n"),
      bindings: [
        { smt_constant: "x", source_expr: "x", source_line: 1, sort: "Bool" },
      ],
    };
    expect(classifyInvariantKind(commentNoDecl)).toBe("abstract");
  });
});

// ---------------------------------------------------------------------------
// 4c. proseJaccardAgreement
// ---------------------------------------------------------------------------

describe("proseJaccardAgreement", () => {
  // The adversary prose prompt contains "INDEPENDENTLY" as a distinctive substring,
  // same as crossLlmAgreement. Distinguish by the sentinel "describe in prose"
  // or by overall text shape; here we just rely on the proseJaccard stage marker.

  const SHELL_INJECTION_SIGNAL: BugSignal = {
    source: "test",
    rawText: "listFiles(input) interpolates input into execSync template; shell metacharacters execute arbitrary commands",
    summary: "shell injection in listFiles via execSync interpolation",
    failureDescription: "input containing shell metacharacters reaches execSync without sanitization",
    codeReferences: [{ file: "cmd.ts", line: 3 }],
    bugClassHint: "shell-injection",
  };

  const ABSTRACT_CLAIM: InvariantClaim = {
    principleId: null,
    description: "input passed to listFiles must not contain shell metacharacters before being interpolated into the execSync command string",
    formalExpression: [
      "(declare-const tainted Bool)",
      "(declare-const sanitized Bool)",
      "(assert (and tainted (not sanitized)))",
      "(check-sat)",
    ].join("\n"),
    bindings: [
      { smt_constant: "tainted", source_expr: "input", source_line: 3, sort: "Bool" },
      { smt_constant: "sanitized", source_expr: "input", source_line: 3, sort: "Bool" },
    ],
    complexity: 1,
    witness: "sat",
    citations: [
      { smt_clause: "(and tainted (not sanitized))", source_quote: "input containing shell metacharacters reaches execSync without sanitization" },
    ],
  };

  it("passes when adversary prose has overlap >= 0.35 of content words", async () => {
    const adversaryProse = JSON.stringify({
      description: "shell metacharacters in input flow into execSync command without sanitization",
    });
    const llm = makeStubLlm(new Map([["INDEPENDENTLY", adversaryProse]]));
    const result = await proseJaccardAgreement({
      invariant: ABSTRACT_CLAIM,
      signal: SHELL_INJECTION_SIGNAL,
      llm,
    });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/PASS/);
    expect(result.detail).toMatch(/skipped SMT cross-LLM for abstract invariant/);
  });

  it("passes on the run-1 evidence pair (verbose adversary prose with 4 of 9 shared)", async () => {
    // Reproduces the pitch-leak-2 evidence: proposer was 9 stemmed content words,
    // adversary was 18, shared 4 (shell, command, allow, injection).
    // Pure Jaccard would give 4/(9+18-4)=0.17 (fails 0.3); overlap is 4/min(9,18)=0.44 (passes 0.35).
    const claim: InvariantClaim = {
      ...ABSTRACT_CLAIM,
      description: "The invariant input passed to execSync must not contain shell metacharacters is violated when input contains an unsanitized shell metacharacter that allows command injection.",
    };
    const adversaryProse = JSON.stringify({
      description: "User-controlled input must be sanitized or escaped before being incorporated into shell commands. Direct interpolation of untrusted input into shell command strings without proper escaping allows arbitrary command execution through injection attacks.",
    });
    const llm = makeStubLlm(new Map([["INDEPENDENTLY", adversaryProse]]));
    const result = await proseJaccardAgreement({
      invariant: claim,
      signal: SHELL_INJECTION_SIGNAL,
      llm,
    });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/PASS/);
  });

  it("fails when adversary prose disagrees (overlap < 0.35)", async () => {
    const adversaryProse = JSON.stringify({
      description: "buffer overflow when memcpy length exceeds destination capacity",
    });
    const llm = makeStubLlm(new Map([["INDEPENDENTLY", adversaryProse]]));
    const result = await proseJaccardAgreement({
      invariant: ABSTRACT_CLAIM,
      signal: SHELL_INJECTION_SIGNAL,
      llm,
    });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/FAIL/);
    expect(result.detail).toMatch(/skipped SMT cross-LLM for abstract invariant/);
  });

  it("fails when adversary call throws", async () => {
    const llm: LLMProvider = { complete: async () => { throw new Error("network"); } };
    const result = await proseJaccardAgreement({
      invariant: ABSTRACT_CLAIM,
      signal: SHELL_INJECTION_SIGNAL,
      llm,
    });
    expect(result.passed).toBe(false);
    expect(result.detail).toContain("network");
  });
});

// ---------------------------------------------------------------------------
// 4d. runInvariantFidelity adaptive routing
// ---------------------------------------------------------------------------

describe("runInvariantFidelity adaptive routing", () => {
  const SHELL_SIGNAL: BugSignal = {
    source: "test",
    rawText: "shell injection via execSync interpolation",
    summary: "shell injection in listFiles",
    failureDescription: "input flows into execSync without sanitization",
    codeReferences: [{ file: "cmd.ts", line: 3 }],
    bugClassHint: "shell-injection",
  };

  const ABSTRACT_CLAIM: InvariantClaim = {
    principleId: null,
    description: "input must be sanitized before reaching execSync",
    formalExpression: [
      "(declare-const tainted Bool)",
      "(declare-const sanitized Bool)",
      "(assert (and tainted (not sanitized)))",
      "(check-sat)",
    ].join("\n"),
    bindings: [
      { smt_constant: "tainted", source_expr: "input", source_line: 3, sort: "Bool" },
      { smt_constant: "sanitized", source_expr: "input", source_line: 3, sort: "Bool" },
    ],
    complexity: 1,
    witness: "sat",
    citations: [
      { smt_clause: "(and tainted (not sanitized))", source_quote: "input flows into execSync without sanitization" },
    ],
  };

  it("for abstract invariant: runs prose-Jaccard + traceability; skips SMT cross-LLM and fixtures", async () => {
    const callLog: string[] = [];
    const verifiers = {
      crossLlmAgreement: async (): Promise<FidelityCheckResult> => { callLog.push("cross"); return { passed: false, detail: "should not run" }; },
      traceabilityCheck: async (): Promise<FidelityCheckResult> => { callLog.push("trace"); return { passed: true, detail: "grounded" }; },
      adversarialFixturePreValidation: async (): Promise<FidelityCheckResult> => { callLog.push("fixture"); return { passed: false, detail: "should not run" }; },
      proseJaccardAgreement: async (): Promise<FidelityCheckResult> => { callLog.push("prose"); return { passed: true, detail: "PASS overlap=0.50 (skipped SMT cross-LLM for abstract invariant)" }; },
    } as unknown as FidelityVerifiers;
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };

    const result = await runInvariantFidelity({
      invariant: ABSTRACT_CLAIM,
      signal: SHELL_SIGNAL,
      llm,
      _verifiers: verifiers,
    });

    expect(result.passed).toBe(true);
    expect(result.invariantKind).toBe("abstract");
    expect(callLog).toContain("prose");
    expect(callLog).toContain("trace");
    expect(callLog).not.toContain("cross");
    expect(callLog).not.toContain("fixture");
  });

  it("for abstract invariant: fails with prose-Jaccard reason when prose disagrees", async () => {
    const verifiers = {
      crossLlmAgreement: async (): Promise<FidelityCheckResult> => { throw new Error("should not run"); },
      traceabilityCheck: async (): Promise<FidelityCheckResult> => ({ passed: true, detail: "grounded" }),
      adversarialFixturePreValidation: async (): Promise<FidelityCheckResult> => { throw new Error("should not run"); },
      proseJaccardAgreement: async (): Promise<FidelityCheckResult> => ({ passed: false, detail: "FAIL overlap=0.10 < 0.40 (skipped SMT cross-LLM for abstract invariant)" }),
    } as unknown as FidelityVerifiers;
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };

    const result = await runInvariantFidelity({
      invariant: ABSTRACT_CLAIM,
      signal: SHELL_SIGNAL,
      llm,
      _verifiers: verifiers,
    });

    expect(result.passed).toBe(false);
    expect(result.invariantKind).toBe("abstract");
    expect(result.failures.length).toBeGreaterThanOrEqual(1);
    expect(result.failures.some((f) => /overlap/i.test(f))).toBe(true);
  });

  it("for concrete invariant: demotes to abstract when fixtures returns 0/N negatives correct (Bool-flag-as-Int encoding)", async () => {
    // v4 dogfood evidence: proposer LLM emits Int decls but with a Bool-flag-
    // shaped assertion (e.g. (assert (= input 1))); fixtures verifier returns
    // 0/5 negatives correct because the assertion is tautologically SAT for
    // every input binding the LLM picks. Orchestrator must demote to abstract
    // and re-route through prose-overlap rather than fail the invariant.
    const callLog: string[] = [];
    const verifiers = {
      crossLlmAgreement: async (): Promise<FidelityCheckResult> => { callLog.push("cross"); return { passed: false, detail: "FAIL semantic disagreement" }; },
      traceabilityCheck: async (): Promise<FidelityCheckResult> => { callLog.push("trace"); return { passed: true, detail: "grounded" }; },
      adversarialFixturePreValidation: async (): Promise<FidelityCheckResult> => { callLog.push("fixture"); return { passed: false, detail: "adversarial fixtures: FAIL — negative fixtures: only 0/5 classified correctly (fixture[0]=positive, fixture[1]=positive, fixture[2]=positive, fixture[3]=positive, fixture[4]=positive)" }; },
      proseJaccardAgreement: async (): Promise<FidelityCheckResult> => { callLog.push("prose"); return { passed: true, detail: "prose overlap: PASS" }; },
    } as unknown as FidelityVerifiers;
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };

    const result = await runInvariantFidelity({
      invariant: VALID_CLAIM, // Int-bound invariant -> classified concrete
      signal: DIVIDE_BUG_SIGNAL,
      llm,
      _verifiers: verifiers,
    });

    // Demoted to abstract: prose-overlap pass + traceability pass = overall pass
    expect(result.passed).toBe(true);
    expect(result.invariantKind).toBe("abstract");
    expect(callLog).toContain("fixture"); // concrete path ran first
    expect(callLog).toContain("prose"); // then demoted and prose ran
  });

  it("for concrete invariant: runs all three checks; prose-Jaccard not called (regression)", async () => {
    const callLog: string[] = [];
    const verifiers = {
      crossLlmAgreement: async (): Promise<FidelityCheckResult> => { callLog.push("cross"); return { passed: true, detail: "ok" }; },
      traceabilityCheck: async (): Promise<FidelityCheckResult> => { callLog.push("trace"); return { passed: true, detail: "ok" }; },
      adversarialFixturePreValidation: async (): Promise<FidelityCheckResult> => { callLog.push("fixture"); return { passed: true, detail: "ok" }; },
      proseJaccardAgreement: async (): Promise<FidelityCheckResult> => { callLog.push("prose"); return { passed: false, detail: "should not run" }; },
    } as unknown as FidelityVerifiers;
    const llm: LLMProvider = { complete: async () => { throw new Error("not called"); } };

    const result = await runInvariantFidelity({
      invariant: VALID_CLAIM, // arithmetic Int-bound invariant from fixtures
      signal: DIVIDE_BUG_SIGNAL,
      llm,
      _verifiers: verifiers,
    });

    expect(result.passed).toBe(true);
    expect(result.invariantKind).toBe("concrete");
    expect(callLog).toContain("cross");
    expect(callLog).toContain("trace");
    expect(callLog).toContain("fixture");
    expect(callLog).not.toContain("prose");
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
