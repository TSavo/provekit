/**
 * Shared canned LLM responses for prompts emitted by the C1.5 invariant-fidelity
 * verifiers (oracle #1.5). Without these, every scenario that runs the novel-LLM
 * path at C1 fails immediately because the stub LLM has no canned answer for the
 * traceability verifier prompt.
 *
 * The three C1.5 verifier prompts:
 *   1. crossLlmAgreement — adversary derivation. Uses prefix
 *      "You are a formal verification expert. Given a bug report, produce an
 *      SMT-LIB assertion" — same as the C1 LLM prompt, so each scenario's own
 *      "formal verification expert" stub satisfies it (the adversary returns
 *      the same invariant as the proposer; mutual entailment passes).
 *   2. traceabilityCheck  — generic verifier. Always returns
 *      `{"all_grounded": true}`. Uses unique substring "Citations to verify".
 *   3. adversarialFixturePreValidation — must classify 5 positive + 5 negative
 *      fixtures correctly against the proposer's SMT. Cannot be shared because
 *      the binding names (b, y, count, userIsNull, ...) and sorts (Int, Bool)
 *      are scenario-specific. Each scenario provides its own stub, keyed on
 *      "Generate 5 POSITIVE fixtures".
 *
 * Common stubs are merged AFTER each scenario's own responses (see runner.ts),
 * so a scenario can override any of these by listing its own matching key
 * earlier.
 */

import type { CorpusScenario } from "./scenarios.js";

/**
 * Stubs for prompts that all scenarios share. Each entry's matchPrompt must be
 * a substring unique enough that it does not collide with other prompts in the
 * pipeline.
 */
export const INVARIANT_FIDELITY_STUBS: CorpusScenario["llmResponses"] = [
  {
    // Traceability verifier prompt: "Citations to verify:\n${citationsJson}"
    // unique to traceabilityCheck.
    matchPrompt: "Citations to verify",
    response: JSON.stringify({ all_grounded: true }),
  },
];

// ---------------------------------------------------------------------------
// Per-scenario adversarial-fixture stub builders
// ---------------------------------------------------------------------------

/**
 * Build a stub for the C1.5 adversarial fixture pre-validation prompt
 * ("Generate 5 POSITIVE fixtures") that classifies correctly against a
 * single-Int-binding violation invariant of the form `(assert (= <name> 0))`.
 *
 * Positive fixtures pin <name> to 0 (SAT). Negative fixtures pin <name> to a
 * nonzero value (UNSAT against the violation).
 */
export function intZeroFixtureStub(constName: string): CorpusScenario["llmResponses"][number] {
  const positives = Array.from({ length: 5 }, (_, i) => ({
    source: `function f_pos_${i}(x: number): number { return x / 0; }`,
    inputBindings: { [constName]: 0 },
    description: `${constName} pinned to zero — bug present`,
  }));
  const negatives = Array.from({ length: 5 }, (_, i) => ({
    source: `function f_neg_${i}(x: number): number { if (${i + 1} === 0) throw new Error("zero"); return x / ${i + 1}; }`,
    inputBindings: { [constName]: i + 1 },
    description: `${constName} = ${i + 1} — clean`,
  }));
  return {
    matchPrompt: "Generate 5 POSITIVE fixtures",
    response: JSON.stringify({ positive: positives, negative: negatives }),
  };
}

/**
 * Build a stub for fixture pre-validation against a single-Bool-binding
 * violation invariant of the form `(assert (= <name> <truthy>))`.
 *
 * `expectedValue` is the value of <name> that triggers the violation
 * (positive fixtures); negative fixtures use the opposite.
 */
export function boolFixtureStub(
  constName: string,
  expectedValue: boolean,
): CorpusScenario["llmResponses"][number] {
  const positives = Array.from({ length: 5 }, (_, i) => ({
    source: `function f_pos_${i}(x: any): any { return x.field; }`,
    inputBindings: { [constName]: expectedValue },
    description: `${constName} = ${expectedValue} — bug present`,
  }));
  const negatives = Array.from({ length: 5 }, (_, i) => ({
    source: `function f_neg_${i}(x: any): any { if (x == null) throw new Error("null"); return x.field; }`,
    inputBindings: { [constName]: !expectedValue },
    description: `${constName} = ${!expectedValue} — clean`,
  }));
  return {
    matchPrompt: "Generate 5 POSITIVE fixtures",
    response: JSON.stringify({ positive: positives, negative: negatives }),
  };
}

/**
 * Build a stub when the invariant uses two Int constants and the violation is
 * `(assert (= a b))` — i.e. the two values collapse together. Positive fixtures
 * pick equal values, negatives pick unequal values.
 */
export function intEqualityFixtureStub(
  constA: string,
  constB: string,
): CorpusScenario["llmResponses"][number] {
  const positives = Array.from({ length: 5 }, (_, i) => ({
    source: `function f_pos_${i}(d: boolean): number { return d ? ${i} : ${i}; }`,
    inputBindings: { [constA]: i, [constB]: i },
    description: `${constA} == ${constB} == ${i} — bug present`,
  }));
  const negatives = Array.from({ length: 5 }, (_, i) => ({
    source: `function f_neg_${i}(d: boolean): number { return d ? ${i} : ${i + 1}; }`,
    inputBindings: { [constA]: i, [constB]: i + 1 },
    description: `${constA} != ${constB} — clean`,
  }));
  return {
    matchPrompt: "Generate 5 POSITIVE fixtures",
    response: JSON.stringify({ positive: positives, negative: negatives }),
  };
}
