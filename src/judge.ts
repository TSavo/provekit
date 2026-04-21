import { LLMProvider } from "./llm";
import { verifyBlock } from "./verifier";

export interface JudgeVerdict {
  valid: boolean;
  note: string;
}

export interface JudgeInput {
  functionSource?: string;
  smt2: string;
  claim: string;
  reason: string;
  expected: "sat" | "unsat";
}

export async function judgeReasoning(
  input: JudgeInput,
  provider: LLMProvider,
  model: string
): Promise<JudgeVerdict> {
  const sourceBlock = input.functionSource
    ? `## Source code\n\`\`\`typescript\n${input.functionSource}\n\`\`\`\n\n`
    : "";

  const prompt = `You are a verification judge. Another LLM emitted an SMT-LIB proof obligation plus a reason explaining why it should hold. Check whether the reason actually justifies the claim, whether the SMT-LIB faithfully encodes the code, and whether the expected Z3 verdict (${input.expected}) is consistent with the reasoning.

${sourceBlock}## Claim
${input.claim}

## Stated reason
${input.reason}

## SMT-LIB encoding
\`\`\`smt2
${input.smt2}
\`\`\`

## Expected Z3 verdict
${input.expected}

## Your task

Reply with exactly one line. Start with either VALID or INVALID, then a colon, then a single short sentence.

- VALID — the reason is coherent, the SMT-LIB tracks real code behaviour, and the expected verdict matches the reasoning.
- INVALID — the reason is circular, unrelated, or the SMT-LIB encodes something the code does not actually do, or the verdict contradicts the reasoning.

Bias toward INVALID when the reason restates the claim without justifying it, when the SMT-LIB invents constants the code does not have, or when the reason appeals to code the snippet doesn't contain.`;

  let resp;
  try {
    resp = await provider.complete(prompt, {
      model,
      systemPrompt:
        "You judge SMT-LIB proof obligations against their stated reasons. Reply with one line starting VALID: or INVALID:.",
    });
  } catch (err: any) {
    return { valid: true, note: `judge-error: ${err?.message?.slice(0, 80) || "unknown"}` };
  }

  const first = resp.text.trim().split("\n")[0] || "";
  const m = first.match(/^(VALID|INVALID)\s*[:\-]\s*(.*)$/i);
  if (!m) {
    return { valid: true, note: `judge-unparseable: ${first.slice(0, 80)}` };
  }
  const valid = m[1]!.toUpperCase() === "VALID";
  const note = m[2]!.trim().slice(0, 200);
  return { valid, note };
}

export interface TeachingExampleInput {
  name: string;
  description: string;
  explanation: string;
  smt2: string;
}

export interface HarnessAuditInput {
  harnessCode: string;
  claim: string;
  smt2: string;
  functionSource: string;
}

export async function judgeHarnessCode(
  input: HarnessAuditInput,
  provider: LLMProvider,
  model: string
): Promise<JudgeVerdict> {
  const prompt = `You are auditing a test harness that another LLM just wrote. The harness will be executed to empirically test whether runtime behaviour matches a formally-proven claim. Your job is to decide whether the harness is a genuine empirical test or whether it rigs the outcome by construction.

A harness is rigged when any of the following hold. Reject (INVALID) if you see any of them:

1. **Function re-implementation.** The harness re-implements the function body inline (e.g. \`const expression = (args) => ...\`) and tests the re-implementation instead of calling \`functionUnderTest\`. The harness is then testing its own copy, not the real code.

2. **Tautological assertion.** \`expect(typeof result).toBe('object')\`, \`if (result !== result) throw\`, \`assert(true)\`. Any execution satisfies these. They provide no falsification power.

3. **Claim-restated expected value.** The \`expected\` value in the assertion is a direct transliteration of the claim (e.g. claim says "returns ok", harness has \`const expected = 'ok'\`). The harness then verifies its own translation agrees with itself.

4. **Fixture contains the answer.** The input fixture carries the value the function is supposed to compute (e.g. claim says "maximum of array", fixture is \`{ data: [1,2,3], max: 3 }\`). The function can read \`.max\` and pass through.

5. **encoding-gap thrown on conditions orthogonal to the claim.** The harness throws \`encoding-gap:\` on a TypeError, a null dereference, or a coincidental runtime failure that doesn't falsify the claim. The prefix should only be used when the observation specifically contradicts what the claim promises.

6. **Hypothesis-assertion drift.** If the harness has a \`// falsification:\` comment naming a specific runtime observation, the assertions below must test for that observation. If the comment says "function mutates X" but assertions only check the return value, the harness is incoherent.

## Claim
${input.claim}

## SMT-LIB (what was formally proven)
\`\`\`smt2
${input.smt2}
\`\`\`

## Function source
\`\`\`typescript
${input.functionSource}
\`\`\`

## Harness to audit
\`\`\`javascript
${input.harnessCode}
\`\`\`

## Your output

Reply with exactly one line, starting VALID or INVALID:

- \`VALID: <one-sentence confirmation that the harness is a genuine empirical test>\`
- \`INVALID: <one-sentence specific critique naming the rigging mechanism — cite which of the six categories above, with a short quote from the harness>\`

Bias toward INVALID. A falsely-validated rigged harness poisons the pipeline's trust signal; a falsely-rejected legitimate harness just means we lose one data point and synthesis is retried.`;

  let resp;
  try {
    resp = await provider.complete(prompt, {
      model,
      systemPrompt:
        "You audit synthesized test harnesses for rigging and tautology. Reply with one line starting VALID: or INVALID:, citing the specific rigging mechanism when rejecting.",
    });
  } catch (err: any) {
    return { valid: true, note: `audit-error: ${err?.message?.slice(0, 80) || "unknown"}` };
  }

  const first = resp.text.trim().split("\n")[0] || "";
  const m = first.match(/^(VALID|INVALID)\s*[:\-]\s*(.*)$/i);
  if (!m) return { valid: true, note: `audit-unparseable: ${first.slice(0, 80)}` };

  const valid = m[1]!.toUpperCase() === "VALID";
  const note = m[2]!.trim().slice(0, 200);
  return { valid, note };
}

export async function judgeTeachingExample(
  ex: TeachingExampleInput,
  provider: LLMProvider,
  model: string
): Promise<JudgeVerdict> {
  const { result } = verifyBlock(ex.smt2);
  if (result !== "sat" && result !== "unsat") {
    return { valid: true, note: `judge-skipped: teaching example z3 result ${result}` };
  }
  return judgeReasoning(
    {
      smt2: ex.smt2,
      claim: `${ex.name}: ${ex.description}`,
      reason: ex.explanation,
      expected: result,
    },
    provider,
    model
  );
}

export interface RuntimeOutcomeInput {
  functionSource: string;
  claim: string;
  smt2: string;
  inputsSummary: string;
  outcome:
    | { kind: "returned"; value: string }
    | { kind: "threw"; error: string };
}

export async function judgeRuntimeOutcome(
  input: RuntimeOutcomeInput,
  provider: LLMProvider,
  model: string
): Promise<JudgeVerdict> {
  const outcomeDesc =
    input.outcome.kind === "returned"
      ? `returned ${input.outcome.value}`
      : `threw: ${input.outcome.error}`;

  const prompt = `Z3 proved a claim about this function (unsat on the negated goal). We then executed the function with concrete inputs sampled from Z3's own model of the preconditions. Your job is to decide whether the runtime outcome is consistent with the claim, or whether it reveals an encoding inconsistency — i.e. Z3 said "proven" but the actual code disagrees with the claim.

## Function source
\`\`\`typescript
${input.functionSource}
\`\`\`

## Claim Z3 proved
${input.claim}

## SMT-LIB encoding Z3 evaluated
\`\`\`smt2
${input.smt2}
\`\`\`

## Concrete inputs (extracted from Z3's model of the preconditions)
${input.inputsSummary}

## Observed runtime outcome
${outcomeDesc}

## Your task

Decide:

- VALID — the observed runtime outcome is consistent with what the claim says should happen for these inputs. The claim is a faithful statement about the code.
- INVALID — the outcome contradicts the claim. Either the SMT-LIB encoding doesn't model the code, or the claim as stated isn't actually true at runtime.

Specifically, INVALID applies when the function threw on an input the claim says should succeed, when it returned a value the claim says was impossible, or when Z3 proved a property that real execution falsifies.

Reply with exactly one line, starting VALID or INVALID followed by a colon and a single short sentence of justification.`;

  let resp;
  try {
    resp = await provider.complete(prompt, {
      model,
      systemPrompt:
        "You judge whether runtime behaviour is consistent with a Z3-proven claim about a function. Reply with one line starting VALID: or INVALID:.",
    });
  } catch (err: any) {
    return { valid: true, note: `judge-error: ${err?.message?.slice(0, 80) || "unknown"}` };
  }

  const first = resp.text.trim().split("\n")[0] || "";
  const m = first.match(/^(VALID|INVALID)\s*[:\-]\s*(.*)$/i);
  if (!m) {
    return { valid: true, note: `judge-unparseable: ${first.slice(0, 80)}` };
  }
  const valid = m[1]!.toUpperCase() === "VALID";
  const note = m[2]!.trim().slice(0, 200);
  return { valid, note };
}
