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
