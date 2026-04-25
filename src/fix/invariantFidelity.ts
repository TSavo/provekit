/**
 * Oracle #1.5 — Invariant Fidelity Check.
 *
 * Three independent mechanical verifiers that confirm an LLM-proposed
 * InvariantClaim is faithful to the prose bug report BEFORE C1 returns it.
 *
 * Fires ONLY on the novel-LLM path (principleId === null). Principle-match
 * invariants are skipped — they were adversarially validated at C6 generation
 * time (oracle #6). Trust inheritance from the principle library is the
 * architectural symmetry that avoids 3 extra LLM calls per already-vetted match.
 *
 * The three verifiers:
 *  1. crossLlmAgreement  — second LLM derives its own invariant from the same
 *                          prose; Z3 implication checks confirm semantic equivalence.
 *  2. traceabilityCheck  — each SMT clause must be cited to a source quote;
 *                          a verifier LLM confirms grounding.
 *  3. adversarialFixturePreValidation — 5 positive + 5 negative TS fixtures;
 *                          SMT classification must hit ≥4/5 in each category.
 *
 * Cost per C1 novel-path call: +25–45 s (adversary derivation ~10–15 s,
 * traceability verifier ~5–10 s, fixture generation ~10–15 s, Z3 ~0.3 s).
 */

import type { InvariantClaim, BugSignal, LLMProvider, InvariantCitation } from "./types.js";
import { createNoopLogger, type FixLoopLogger } from "./logger.js";
import { requestStructuredJson } from "./llm/structuredOutput.js";
import { verifyBlock } from "../verifier.js";

// ---------------------------------------------------------------------------
// Shared result shape
// ---------------------------------------------------------------------------

export interface FidelityCheckResult {
  passed: boolean;
  detail: string;
}

// ---------------------------------------------------------------------------
// Helper: pick the adversary model tier
// ---------------------------------------------------------------------------

function adversaryModel(proposerModel: "opus" | "sonnet" | "haiku"): "opus" | "sonnet" {
  // Different tier than the proposer; haiku proposer treated like sonnet for this purpose.
  return proposerModel === "opus" ? "sonnet" : "opus";
}

// ---------------------------------------------------------------------------
// 1. Cross-LLM Agreement
// ---------------------------------------------------------------------------

/**
 * Ask a second LLM to derive its OWN invariant from the same prose bug report,
 * then confirm semantic equivalence with Z3 implication checks.
 *
 * Alpha-renaming: the adversary's SMT constants are matched to the proposer's
 * by source_expr (the human-readable expression, e.g. "b"). If the binding
 * sets differ in size, the smaller set is the constraint domain and extras are
 * treated as free variables (DEGRADE: text-similarity fallback on mismatch).
 */
export async function crossLlmAgreement(args: {
  invariant: InvariantClaim;
  signal: BugSignal;
  llm: LLMProvider;
  logger?: FixLoopLogger;
}): Promise<FidelityCheckResult> {
  const { invariant, signal, llm, logger = createNoopLogger() } = args;

  const adversaryPrompt = `You are a formal verification expert. Given a bug report, produce an SMT-LIB assertion
that expresses the VIOLATION STATE (the negation of the desired invariant).
The assertion must be satisfiable (Z3 check-sat returns "sat") — this proves the bug is reachable.

Bug summary: ${signal.summary}
Failure description: ${signal.failureDescription}

IMPORTANT: Derive the invariant INDEPENDENTLY from the prose above.
Do NOT adjust to match any other invariant you may have seen.

Respond with ONLY a JSON object (no markdown fences):
{
  "description": "one sentence: what invariant is being violated",
  "smt_declarations": ["(declare-const varName Sort)", ...],
  "smt_violation_assertion": "(assert (...))",
  "bindings": [
    {"smt_constant": "varName", "source_expr": "expression in source", "sort": "Int"}
  ]
}

Rules:
- smt_declarations: declare all constants you use
- smt_violation_assertion: a single (assert ...) encoding the violation state
- Do NOT include (check-sat) — it will be appended automatically
- Use Int or Bool sorts
- Keep it simple: 2-5 constants maximum
- Each binding's source_expr must be a literal source identifier (e.g. "b", "a", "x")`;

  const proposerModel = "opus"; // proposer is always opus on the novel path (C1 default)
  const adModel = adversaryModel(proposerModel);

  // Parse adversary response
  interface AdversaryResponse {
    description: string;
    smt_declarations: string[];
    smt_violation_assertion: string;
    bindings: { smt_constant: string; source_expr: string; sort: string }[];
  }

  let adversary: AdversaryResponse;
  try {
    adversary = await requestStructuredJson<AdversaryResponse>({
      prompt: adversaryPrompt,
      llm,
      stage: "C1.5-crossLLM",
      model: adModel,
      logger,
    });
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    return { passed: false, detail: `cross-LLM agreement: adversary call/parse failed — ${msg}` };
  }

  if (!adversary.smt_declarations || !adversary.smt_violation_assertion || !adversary.bindings) {
    return { passed: false, detail: "cross-LLM agreement: adversary response missing required fields" };
  }

  // Build alpha-rename map: adversary source_expr → proposer smt_constant
  // so we can translate adversary's SMT into proposer's namespace.
  const proposerBySourceExpr = new Map<string, string>();
  for (const b of invariant.bindings) {
    proposerBySourceExpr.set(b.source_expr, b.smt_constant);
  }

  // Check if we can align the binding sets
  const adversaryBindings = adversary.bindings;
  let canAlignAll = true;
  const renameMap = new Map<string, string>(); // adversary smt_constant → proposer smt_constant

  for (const ab of adversaryBindings) {
    const proposerConst = proposerBySourceExpr.get(ab.source_expr);
    if (proposerConst) {
      renameMap.set(ab.smt_constant, proposerConst);
    } else {
      canAlignAll = false;
    }
  }

  if (!canAlignAll) {
    // DEGRADE: text-similarity fallback on binding mismatch.
    // Compare descriptions. Two descriptions with >50% word overlap → pass with caveat.
    const proposerWords = new Set(invariant.description.toLowerCase().split(/\W+/).filter(Boolean));
    const adversaryWords = adversary.description.toLowerCase().split(/\W+/).filter(Boolean);
    const overlap = adversaryWords.filter((w) => proposerWords.has(w)).length;
    const ratio = overlap / Math.max(adversaryWords.length, 1);
    const degradeNote = `(DEGRADED: binding sets differ — fell back to text similarity; ratio=${ratio.toFixed(2)})`;
    if (ratio >= 0.5) {
      return {
        passed: true,
        detail: `cross-LLM agreement: PASS ${degradeNote}. proposer="${invariant.description}" adversary="${adversary.description}"`,
      };
    }
    return {
      passed: false,
      detail: `cross-LLM agreement: FAIL ${degradeNote}. Low description similarity. proposer="${invariant.description}" adversary="${adversary.description}"`,
    };
  }

  // Alpha-rename: replace adversary constant names with proposer constant names
  function alphaRename(smtText: string): string {
    let result = smtText;
    // Sort by length desc to avoid partial-rename collisions (e.g. "bb" → "b")
    const entries = [...renameMap.entries()].sort((a, b) => b[0].length - a[0].length);
    for (const [adversaryConst, proposerConst] of entries) {
      // Replace whole-word occurrences: SMT identifiers are delimited by parens/spaces
      result = result.replace(new RegExp(`\\b${adversaryConst}\\b`, "g"), proposerConst);
    }
    return result;
  }

  const adversaryDecls = adversary.smt_declarations.map(alphaRename);
  const adversaryAssertion = alphaRename(adversary.smt_violation_assertion);

  // Extract proposer assertion (strip declarations and check-sat from formalExpression)
  const proposerLines = invariant.formalExpression.split("\n");
  const proposerAssertionLines = proposerLines.filter(
    (l) => l.trim().startsWith("(assert"),
  );
  if (proposerAssertionLines.length === 0) {
    return { passed: false, detail: "cross-LLM agreement: could not extract proposer assertion from formalExpression" };
  }
  const proposerAssertion = proposerAssertionLines.join("\n");

  // Extract proposer declarations
  const proposerDeclLines = proposerLines.filter((l) => l.trim().startsWith("(declare-const"));

  // Merge all declarations (both sets, deduplicated by constant name)
  const allDecls = new Map<string, string>(); // constant → full decl line
  for (const d of [...proposerDeclLines, ...adversaryDecls]) {
    const m = d.match(/\(declare-const\s+(\S+)/);
    if (m && m[1]) allDecls.set(m[1], d);
  }
  const mergedDecls = [...allDecls.values()].join("\n");

  // Direction 1: proposer's violation ∧ ¬adversary's violation → UNSAT means proposer ⊆ adversary
  const smt1 = `${mergedDecls}
${proposerAssertion}
(assert (not ${adversaryAssertion.replace(/^\(assert\s+/, "").replace(/\)$/, ")")}))
(check-sat)`;

  // Direction 2: adversary's violation ∧ ¬proposer's violation → UNSAT means adversary ⊆ proposer
  const smt2 = `${mergedDecls}
${adversaryAssertion}
(assert (not ${proposerAssertion.replace(/^\(assert\s+/, "").replace(/\)$/, ")")}))
(check-sat)`;

  logger.detail(`C1.5 crossLLM: direction-1 SMT: ${smt1.slice(0, 300)}`);
  logger.detail(`C1.5 crossLLM: direction-2 SMT: ${smt2.slice(0, 300)}`);

  const r1 = verifyBlock(smt1);
  const r2 = verifyBlock(smt2);

  logger.detail(`C1.5 crossLLM: dir1=${r1.result} dir2=${r2.result}`);

  if (r1.result === "unsat" && r2.result === "unsat") {
    return {
      passed: true,
      detail: `cross-LLM agreement: PASS — mutual entailment (both directions UNSAT). proposer="${invariant.description}" adversary="${adversary.description}"`,
    };
  }

  const failDir: string[] = [];
  if (r1.result !== "unsat") failDir.push(`dir1=${r1.result} (proposer⊄adversary)`);
  if (r2.result !== "unsat") failDir.push(`dir2=${r2.result} (adversary⊄proposer)`);

  return {
    passed: false,
    detail: `cross-LLM agreement: FAIL — ${failDir.join(", ")}. LLMs disagree on violation semantics. proposer="${invariant.description}" adversary="${adversary.description}"`,
  };
}

// ---------------------------------------------------------------------------
// 2. Traceability Check
// ---------------------------------------------------------------------------

/**
 * Confirm that each citation's source_quote is actually present in the bug
 * report. A verifier LLM (opposite tier) re-reads the report + citation list
 * and returns a grounding verdict for each clause.
 */
export async function traceabilityCheck(args: {
  invariant: InvariantClaim;
  signal: BugSignal;
  llm: LLMProvider;
  logger?: FixLoopLogger;
}): Promise<FidelityCheckResult> {
  const { invariant, signal, llm, logger = createNoopLogger() } = args;

  const citations = invariant.citations;
  if (!citations || citations.length === 0) {
    return {
      passed: false,
      detail: `traceability: FAIL — invariant has no citations; cannot confirm grounding. Add a 'citations' field to the LLM response.`,
    };
  }

  const citationsJson = JSON.stringify(citations, null, 2);
  const bugReportText = [signal.summary, signal.failureDescription, signal.rawText]
    .filter(Boolean)
    .join("\n\n");

  const verifierPrompt = `You are a verification expert. A bug report and a list of SMT clause citations are given.
Your task: for each citation, determine whether the "source_quote" is genuinely grounded in the bug report
(the quote must be present verbatim or as a close paraphrase — NOT speculative).

Bug report:
---
${bugReportText}
---

Citations to verify:
${citationsJson}

Respond with ONLY a JSON object (no markdown fences):
{
  "all_grounded": true
}
OR
{
  "all_grounded": false,
  "ungrounded": [
    {"smt_clause": "...", "reason": "quote not found or speculative"}
  ]
}`;

  interface VerifierResponse {
    all_grounded: boolean;
    ungrounded?: { smt_clause: string; reason: string }[];
  }

  let verdict: VerifierResponse;
  try {
    verdict = await requestStructuredJson<VerifierResponse>({
      prompt: verifierPrompt,
      llm,
      stage: "C1.5-traceability",
      model: "sonnet", // opposite tier from opus proposer
      logger,
    });
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    return { passed: false, detail: `traceability: verifier call/parse failed — ${msg}` };
  }

  if (verdict.all_grounded) {
    return {
      passed: true,
      detail: `traceability: PASS — all ${citations.length} citation(s) grounded in bug report`,
    };
  }

  const ungrounded = verdict.ungrounded ?? [];
  const summary = ungrounded
    .map((u) => `clause="${u.smt_clause}" reason="${u.reason}"`)
    .join("; ");

  return {
    passed: false,
    detail: `traceability: FAIL — ${ungrounded.length} ungrounded clause(s): ${summary}`,
  };
}

// ---------------------------------------------------------------------------
// 3. Adversarial Fixture Pre-Validation
// ---------------------------------------------------------------------------

/**
 * Generate 5 positive + 5 negative TS fixtures. For each, classify by
 * substituting the fixture's inputBindings into the violation SMT and checking
 * SAT (SAT → positive; UNSAT → negative).
 *
 * Pass threshold: ≥4/5 in each category.
 */
export async function adversarialFixturePreValidation(args: {
  invariant: InvariantClaim;
  signal: BugSignal;
  llm: LLMProvider;
  logger?: FixLoopLogger;
}): Promise<FidelityCheckResult> {
  const { invariant, signal, llm, logger = createNoopLogger() } = args;

  const fixturePrompt = `You are a software testing expert. Given an invariant and a bug report, generate TypeScript fixtures.

Bug summary: ${signal.summary}
Invariant description: ${invariant.description}
Formal violation SMT: ${invariant.formalExpression}
Bindings (SMT variable → source expression): ${JSON.stringify(invariant.bindings.map((b) => ({ smt_constant: b.smt_constant, source_expr: b.source_expr })))}

Generate 5 POSITIVE fixtures (code that EXHIBITS the bug) and 5 NEGATIVE fixtures (similar but CLEAN code).
For each fixture, also provide the concrete input values for each SMT binding that demonstrate the classification.

Respond with ONLY a JSON object (no markdown fences):
{
  "positive": [
    {
      "source": "function divide(a: number, b: number) { return a / b; }",
      "inputBindings": {"b": 0, "a": 5},
      "description": "b is zero"
    }
  ],
  "negative": [
    {
      "source": "function divide(a: number, b: number) { if (b === 0) throw new Error('zero'); return a / b; }",
      "inputBindings": {"b": 1, "a": 5},
      "description": "guard prevents division by zero"
    }
  ]
}

Rules:
- Each fixture must be a complete, self-contained TypeScript snippet (no imports needed)
- inputBindings must provide a value for every SMT constant in the invariant's bindings
- Positive: inputBindings should make the violation SMT SAT
- Negative: inputBindings should make the violation SMT UNSAT`;

  interface Fixture {
    source: string;
    inputBindings: Record<string, number | boolean>;
    description: string;
  }
  interface FixtureResponse {
    positive: Fixture[];
    negative: Fixture[];
  }

  let fixtures: FixtureResponse;
  try {
    fixtures = await requestStructuredJson<FixtureResponse>({
      prompt: fixturePrompt,
      llm,
      stage: "C1.5-fixtures",
      model: "opus",
      logger,
    });
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    return { passed: false, detail: `adversarial fixtures: LLM call/parse failed — ${msg}` };
  }

  if (!Array.isArray(fixtures.positive) || !Array.isArray(fixtures.negative)) {
    return { passed: false, detail: "adversarial fixtures: fixture response missing 'positive' or 'negative' arrays" };
  }

  // SMT classification: substitute inputBindings into the violation SMT and check SAT.
  // SAT → positive (bug present); UNSAT → negative (bug absent).
  function classifyFixture(
    fixture: Fixture,
    invariantFormal: string,
    bindings: InvariantClaim["bindings"],
  ): "positive" | "negative" | "error" {
    // Extract the base declarations and assertion (strip check-sat)
    const lines = invariantFormal.split("\n").filter((l) => !l.trim().startsWith("(check-sat)"));

    // Build additional assertions for each inputBinding
    const extraAssertions: string[] = [];
    for (const [smtConst] of Object.entries(fixture.inputBindings)) {
      // Verify this constant is in the invariant's bindings
      const matchingBinding = bindings.find((b) => b.smt_constant === smtConst);
      if (!matchingBinding) continue;

      const val = fixture.inputBindings[smtConst];
      if (typeof val === "number") {
        extraAssertions.push(`(assert (= ${smtConst} ${val}))`);
      } else if (typeof val === "boolean") {
        extraAssertions.push(`(assert (= ${smtConst} ${val}))`);
      }
    }

    const smtScript = [...lines, ...extraAssertions, "(check-sat)"].join("\n");
    try {
      const result = verifyBlock(smtScript);
      if (result.result === "sat") return "positive";
      if (result.result === "unsat") return "negative";
      return "error";
    } catch {
      return "error";
    }
  }

  // Evaluate positive fixtures: expect "positive" classification
  const positiveResults = fixtures.positive.slice(0, 5).map((f) =>
    classifyFixture(f, invariant.formalExpression, invariant.bindings),
  );
  const positiveCorrect = positiveResults.filter((r) => r === "positive").length;

  // Evaluate negative fixtures: expect "negative" classification
  const negativeResults = fixtures.negative.slice(0, 5).map((f) =>
    classifyFixture(f, invariant.formalExpression, invariant.bindings),
  );
  const negativeCorrect = negativeResults.filter((r) => r === "negative").length;

  const THRESHOLD = 4;
  const posTotal = Math.min(fixtures.positive.length, 5);
  const negTotal = Math.min(fixtures.negative.length, 5);

  logger.detail(
    `C1.5 fixtures: positive=${positiveCorrect}/${posTotal} correct; negative=${negativeCorrect}/${negTotal} correct`,
  );

  const failures: string[] = [];
  if (positiveCorrect < THRESHOLD) {
    const details = positiveResults
      .map((r, i) => `fixture[${i}]=${r}`)
      .join(", ");
    failures.push(`positive fixtures: only ${positiveCorrect}/${posTotal} classified correctly (${details})`);
  }
  if (negativeCorrect < THRESHOLD) {
    const details = negativeResults
      .map((r, i) => `fixture[${i}]=${r}`)
      .join(", ");
    failures.push(`negative fixtures: only ${negativeCorrect}/${negTotal} classified correctly (${details})`);
  }

  if (failures.length > 0) {
    return {
      passed: false,
      detail: `adversarial fixtures: FAIL — ${failures.join("; ")}`,
    };
  }

  return {
    passed: true,
    detail: `adversarial fixtures: PASS — positive=${positiveCorrect}/${posTotal}, negative=${negativeCorrect}/${negTotal}`,
  };
}

// ---------------------------------------------------------------------------
// runInvariantFidelity orchestration
// ---------------------------------------------------------------------------

export interface InvariantFidelityResult {
  passed: boolean;
  failures: string[];
}

/** Dependency-injected verifiers for testing. Default to the real implementations. */
export interface FidelityVerifiers {
  crossLlmAgreement?: typeof crossLlmAgreement;
  traceabilityCheck?: typeof traceabilityCheck;
  adversarialFixturePreValidation?: typeof adversarialFixturePreValidation;
}

/**
 * Run all three fidelity verifiers. Never short-circuits — full audit trail
 * even on partial failures so the retry prompt has concrete detail.
 *
 * Only runs on novel-LLM-path invariants (principleId === null).
 * Returns { passed: true, failures: [] } immediately for principle-match invariants.
 */
export async function runInvariantFidelity(args: {
  invariant: InvariantClaim;
  signal: BugSignal;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  _verifiers?: FidelityVerifiers;
}): Promise<InvariantFidelityResult> {
  const { invariant, signal, llm, _verifiers } = args;
  const logger = args.logger ?? createNoopLogger();

  // Skip for principle-match path — vetted at C6 generation time.
  if (invariant.principleId !== null) {
    logger.oracle({
      id: 1.5,
      name: "invariant fidelity",
      passed: true,
      detail: `skipped (principle-match path; principleId=${invariant.principleId})`,
    });
    return { passed: true, failures: [] };
  }

  const failures: string[] = [];

  // Resolve verifiers (DI for tests, real implementations by default)
  const crossLlmFn = _verifiers?.crossLlmAgreement ?? crossLlmAgreement;
  const traceabilityFn = _verifiers?.traceabilityCheck ?? traceabilityCheck;
  const fixtureFn = _verifiers?.adversarialFixturePreValidation ?? adversarialFixturePreValidation;

  // Run all three — don't short-circuit — full audit trail
  const [a, b, c] = await Promise.all([
    crossLlmFn({ invariant, signal, llm, logger }),
    traceabilityFn({ invariant, signal, llm, logger }),
    fixtureFn({ invariant, signal, llm, logger }),
  ]);

  if (!a.passed) failures.push(`cross-LLM agreement: ${a.detail}`);
  if (!b.passed) failures.push(`traceability: ${b.detail}`);
  if (!c.passed) failures.push(`adversarial fixtures: ${c.detail}`);

  const passed = failures.length === 0;
  logger.oracle({
    id: 1.5,
    name: "invariant fidelity",
    passed,
    detail: passed
      ? "all three verifiers passed"
      : `${failures.length} failure(s): ${failures.join(" | ")}`,
  });

  return { passed, failures };
}
