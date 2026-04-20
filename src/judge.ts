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
