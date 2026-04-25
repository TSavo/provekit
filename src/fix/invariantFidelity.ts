/**
 * Oracle #1.5 — Invariant Fidelity Check.
 *
 * Mechanical verifiers that confirm an LLM-proposed InvariantClaim is faithful
 * to the prose bug report BEFORE C1 returns it.
 *
 * Fires ONLY on the novel-LLM path (principleId === null). Principle-match
 * invariants are skipped, they were adversarially validated at C6 generation
 * time (oracle #6). Trust inheritance from the principle library is the
 * architectural symmetry that avoids extra LLM calls per already-vetted match.
 *
 * Adaptive routing by invariant kind:
 *  - CONCRETE (arithmetic-style: Int/Real bindings, division-by-zero, off-by-one)
 *      runs three verifiers:
 *       1. crossLlmAgreement, second LLM derives its own invariant; Z3
 *          implication checks confirm semantic equivalence.
 *       2. traceabilityCheck, verifier LLM confirms each SMT clause is grounded
 *          in a source quote.
 *       3. adversarialFixturePreValidation, 5 positive + 5 negative TS fixtures;
 *          SMT classification must hit at least 4/5 in each category.
 *  - ABSTRACT (taint-style: only Bool bindings, no Int/Real; "X flows to Y"
 *      where Z3 has no canonical numerical shape)
 *      runs:
 *       1. proseJaccardAgreement (overlap-coefficient under the hood; name
 *          retained for DI backward compat). Adversary still derives prose
 *          for the same bug; |A∩B|/min(|A|,|B|) over stemmed content words
 *          must be at least 0.4 and the shorter side must have at least 3
 *          content words. SMT cross-LLM is skipped because Bool-only
 *          formalizations are not structurally meaningful (sonnet wrote
 *          (and X (not X) X) for shell injection; Z3 correctly flagged false
 *          but the prose was sound).
 *       2. traceabilityCheck (unchanged).
 *       3. fixtures are skipped, classification without a real taint principle
 *          is meaningless because clean and buggy code share the same Bool shape.
 *
 * Kind detection: see classifyInvariantKind() below.
 *
 * Cost per C1 novel-path call:
 *   concrete: +25-45s (adversary ~10-15s, traceability ~5-10s, fixtures ~10-15s)
 *   abstract: +15-25s (adversary ~10-15s, traceability ~5-10s)
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
// 4. Prose Jaccard Agreement (abstract-invariant path)
// ---------------------------------------------------------------------------

/**
 * Stop-words filtered out before Jaccard similarity. Conservative list:
 * grammatical articles, auxiliaries, prepositions, plus the AI-prose filler
 * ("must", "violated", "input", "passed", "before", "after", "should") that
 * appears in nearly every invariant description regardless of semantic
 * content. The signal lives in the domain nouns ("shell", "metacharacters",
 * "command", "sanitization") that survive the filter.
 */
const STOP_WORDS = new Set<string>([
  "a", "an", "and", "are", "as", "at", "be", "been", "before", "being",
  "by", "for", "from", "had", "has", "have", "in", "is", "it", "its",
  "may", "must", "not", "of", "on", "or", "should", "so", "that", "the",
  "this", "to", "violated", "violation", "was", "were", "will", "with",
  "would", "after", "any", "but", "do", "does", "did", "if", "into",
  "such", "than", "then", "there", "these", "those", "when", "where",
  "which", "who", "whom", "whose", "why", "passed", "input", "value",
  "values", "code", "without", "via",
]);

/**
 * Lightweight stemmer. Not a real Porter stemmer; just enough to collapse the
 * inflectional variants that show up in invariant prose ("contain"/"containing",
 * "metacharacter"/"metacharacters", "interpolat"/"interpolation") so
 * semantically identical descriptions register as the same set element.
 * Without stemming, the run-1 shell-injection prose pair has 4 of 9 content
 * words shared (overlap 0.44); without stemming it drops to 3 of 9 (overlap
 * 0.33) because "containing" and "contain" miss.
 */
function stem(w: string): string {
  if (w.length <= 4) return w;
  if (w.endsWith("ing") && w.length > 5) return w.slice(0, -3);
  if (w.endsWith("ed") && w.length > 4) return w.slice(0, -2);
  if (w.endsWith("es") && w.length > 4) return w.slice(0, -2);
  if (w.endsWith("s") && w.length > 4) return w.slice(0, -1);
  return w;
}

function tokenizeContentWords(text: string): Set<string> {
  const words = text.toLowerCase().split(/\W+/).filter(Boolean);
  return new Set(words.filter((w) => w.length > 1 && !STOP_WORDS.has(w)).map(stem));
}

/**
 * Overlap coefficient (Szymkiewicz-Simpson): |A ∩ B| / min(|A|, |B|).
 *
 * Chosen over Jaccard because invariant prose is asymmetric in length. The
 * adversary tends to write 2-3x more words than the proposer (full sentences
 * with attacker-perspective framing: "user-controlled, untrusted, arbitrary,
 * attacks") while the proposer writes a tight property statement. Jaccard's
 * union denominator punishes that asymmetry; on the run-1 shell-injection
 * case the proposer had 9 stem-words and the adversary had 18, with 4-shared,
 * giving jaccard=0.17. The overlap coefficient on the same case is 0.44,
 * which correctly indicates "they are talking about the same bug, the
 * adversary is just more verbose."
 *
 * The min() denominator means: if every content word in the shorter
 * description shows up in the longer one, overlap=1.0. The shorter side is
 * almost always the proposer (the property statement). False positives are
 * gated by the threshold and by the requirement that the SHORTER side has at
 * least 3 content words (a one-word match doesn't count as agreement).
 */
function overlapCoefficient(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 || b.size === 0) return 0;
  let intersection = 0;
  for (const w of a) if (b.has(w)) intersection++;
  return intersection / Math.min(a.size, b.size);
}

/**
 * Abstract-invariant analog of crossLlmAgreement. Asks the adversary LLM to
 * derive its own invariant prose from the same bug report, then compares
 * content-word sets via the overlap coefficient (Szymkiewicz-Simpson).
 *
 * Threshold: 0.4 with a minimum-length floor on the shorter side (>= 3
 * content words) to gate trivial 1/1 matches. Calibrated empirically against
 * the run-1 shell-injection prose pair (overlap=0.44) and the contrast
 * case shell-injection vs buffer-overflow (overlap=0.0).
 *
 * Skips the SMT round-trip entirely. For Bool-only formalizations Z3 has
 * nothing useful to say; sonnet's (and X (not X) X) was tautologically false
 * not because the invariant was wrong but because Bool-encoding of taint flow
 * is shapeless without a concrete principle to anchor it.
 *
 * Function name retained for backward compat with FidelityVerifiers DI even
 * though the math is no longer Jaccard.
 */
export async function proseJaccardAgreement(args: {
  invariant: InvariantClaim;
  signal: BugSignal;
  llm: LLMProvider;
  logger?: FixLoopLogger;
}): Promise<FidelityCheckResult> {
  const { invariant, signal, llm, logger = createNoopLogger() } = args;

  const adversaryPrompt = `You are a formal verification expert. Given a bug report, describe in prose
the invariant that the buggy code violates. Focus on the property that, if held,
would prevent the bug.

Bug summary: ${signal.summary}
Failure description: ${signal.failureDescription}

IMPORTANT: Derive the invariant INDEPENDENTLY from the prose above.
Do NOT adjust to match any other invariant you may have seen.

Respond with ONLY a JSON object (no markdown fences):
{
  "description": "one to three sentences describing the invariant in plain English"
}`;

  const proposerModel = "opus";
  const adModel = adversaryModel(proposerModel);

  interface ProseResponse {
    description: string;
  }

  let adversary: ProseResponse;
  try {
    adversary = await requestStructuredJson<ProseResponse>({
      prompt: adversaryPrompt,
      llm,
      stage: "C1.5-proseOverlap",
      model: adModel,
      logger,
    });
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    return { passed: false, detail: `prose overlap: adversary call/parse failed (skipped SMT cross-LLM for abstract invariant): ${msg}` };
  }

  if (!adversary.description) {
    return {
      passed: false,
      detail: `prose overlap: adversary response missing description (skipped SMT cross-LLM for abstract invariant)`,
    };
  }

  const proposerWords = tokenizeContentWords(invariant.description);
  const adversaryWords = tokenizeContentWords(adversary.description);
  const minSize = Math.min(proposerWords.size, adversaryWords.size);
  const ratio = overlapCoefficient(proposerWords, adversaryWords);
  const ratioStr = ratio.toFixed(2);
  const THRESHOLD = 0.4;
  const MIN_SHORTER_SIZE = 3;

  logger.detail(
    `C1.5 proseOverlap: proposer=${[...proposerWords].join(",")} adversary=${[...adversaryWords].join(",")} overlap=${ratioStr} minSize=${minSize}`,
  );

  if (minSize < MIN_SHORTER_SIZE) {
    return {
      passed: false,
      detail: `prose overlap: FAIL (skipped SMT cross-LLM for abstract invariant). shorter side has only ${minSize} content word(s); needs >= ${MIN_SHORTER_SIZE}. proposer="${invariant.description}" adversary="${adversary.description}"`,
    };
  }

  if (ratio >= THRESHOLD) {
    return {
      passed: true,
      detail: `prose overlap: PASS (skipped SMT cross-LLM for abstract invariant). overlap=${ratioStr} >= ${THRESHOLD}. proposer="${invariant.description}" adversary="${adversary.description}"`,
    };
  }

  return {
    passed: false,
    detail: `prose overlap: FAIL (skipped SMT cross-LLM for abstract invariant). overlap=${ratioStr} < ${THRESHOLD}. LLMs disagree on what the bug is. proposer="${invariant.description}" adversary="${adversary.description}"`,
  };
}

// ---------------------------------------------------------------------------
// Invariant kind classification
// ---------------------------------------------------------------------------

export type InvariantKind = "concrete" | "abstract";

/**
 * Classify an invariant by its SMT formalization.
 *
 * ABSTRACT: no actual numeric Int/Real declarations in the SMT. These are
 * taint-style invariants ("X flows to Y", "input is sanitized") where Z3
 * has nothing useful to evaluate; the body is shaped like (assert tainted)
 * or (assert (and tainted (not sanitized))) with Bool-only declarations.
 *
 * CONCRETE: at least one (declare-const ... Int) or (declare-const ... Real)
 * in the SMT body. Arithmetic-style invariants (division-by-zero,
 * off-by-one, integer overflow) where SMT has canonical numerical shapes
 * and fixture-classification works.
 *
 * The ground truth is the actual SMT body, not the bindings list. The
 * formulateInvariant parser defaults missing `sort` fields to "Int" (line
 * 403 of stages/formulateInvariant.ts), so LLM-omitted sort metadata cannot
 * be trusted to distinguish a Bool-encoded taint invariant from a real Int
 * arithmetic one. The declare-const lines never lie because Z3 must parse
 * them to run check-sat at all.
 */
export function classifyInvariantKind(invariant: InvariantClaim): InvariantKind {
  // Match (declare-const NAME Int) or (declare-const NAME Real) in the SMT body.
  const numericDeclRe = /\(declare-const\s+\S+\s+(Int|Real)\b/;
  const hasNumericDecl = numericDeclRe.test(invariant.formalExpression);
  if (!hasNumericDecl) return "abstract";
  return "concrete";
}

// ---------------------------------------------------------------------------
// runInvariantFidelity orchestration
// ---------------------------------------------------------------------------

export interface InvariantFidelityResult {
  passed: boolean;
  failures: string[];
  /** Which routing path was taken; surfaced for debugging visibility. */
  invariantKind?: InvariantKind;
}

/** Dependency-injected verifiers for testing. Default to the real implementations. */
export interface FidelityVerifiers {
  crossLlmAgreement?: typeof crossLlmAgreement;
  traceabilityCheck?: typeof traceabilityCheck;
  adversarialFixturePreValidation?: typeof adversarialFixturePreValidation;
  proseJaccardAgreement?: typeof proseJaccardAgreement;
}

/**
 * Run fidelity verifiers, routed by invariant kind. Never short-circuits.
 * Full audit trail even on partial failures so the retry prompt has concrete
 * detail.
 *
 * Routing:
 *   concrete → [crossLlmAgreement, traceabilityCheck, adversarialFixturePreValidation]
 *   abstract → [proseJaccardAgreement, traceabilityCheck]
 *              (SMT cross-LLM and adversarial fixtures skipped, both are
 *               structurally meaningless for Bool-only / non-numeric
 *               formalizations of taint-style invariants.)
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

  // Resolve verifiers (DI for tests, real implementations by default)
  const crossLlmFn = _verifiers?.crossLlmAgreement ?? crossLlmAgreement;
  const traceabilityFn = _verifiers?.traceabilityCheck ?? traceabilityCheck;
  const fixtureFn = _verifiers?.adversarialFixturePreValidation ?? adversarialFixturePreValidation;
  const proseJaccardFn = _verifiers?.proseJaccardAgreement ?? proseJaccardAgreement;

  const kind = classifyInvariantKind(invariant);
  logger.detail(`C1.5 routing: invariantKind=${kind}`);

  const failures: string[] = [];

  if (kind === "concrete") {
    // Existing three-check path for arithmetic-style invariants.
    const [a, b, c] = await Promise.all([
      crossLlmFn({ invariant, signal, llm, logger }),
      traceabilityFn({ invariant, signal, llm, logger }),
      fixtureFn({ invariant, signal, llm, logger }),
    ]);

    if (!a.passed) failures.push(`cross-LLM agreement: ${a.detail}`);
    if (!b.passed) failures.push(`traceability: ${b.detail}`);
    if (!c.passed) failures.push(`adversarial fixtures: ${c.detail}`);
  } else {
    // Abstract path: prose Jaccard + traceability only. SMT cross-LLM and
    // fixtures are intentionally skipped (see header doc).
    const [a, b] = await Promise.all([
      proseJaccardFn({ invariant, signal, llm, logger }),
      traceabilityFn({ invariant, signal, llm, logger }),
    ]);

    if (!a.passed) failures.push(`prose overlap: ${a.detail}`);
    if (!b.passed) failures.push(`traceability: ${b.detail}`);
  }

  const passed = failures.length === 0;
  const checksRun = kind === "concrete" ? "three verifiers" : "prose-overlap + traceability (skipped SMT cross-LLM for abstract invariant; skipped fixtures)";
  logger.oracle({
    id: 1.5,
    name: "invariant fidelity",
    passed,
    detail: passed
      ? `all checks passed (kind=${kind}, ran ${checksRun})`
      : `${failures.length} failure(s) (kind=${kind}, ran ${checksRun}): ${failures.join(" | ")}`,
  });

  return { passed, failures, invariantKind: kind };
}
