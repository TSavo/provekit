import { LLMProvider } from "./llm";
import { verifyBlock } from "./verifier";

export interface RefineResult {
  smt2: string;
  result: "sat" | "unsat" | "unknown" | "error";
}

export async function refineErrorBlock(params: {
  smt2: string;
  z3Error: string;
  claim: string;
  provider: LLMProvider;
  model: string;
}): Promise<RefineResult | null> {
  const { smt2, z3Error, claim, provider, model } = params;

  const prompt = `Z3 failed to evaluate this SMT-LIB block. Produce a corrected block that Z3 can actually run.

## Original block
\`\`\`smt2
${smt2}
\`\`\`

## Z3 error / unknown reason
\`\`\`
${z3Error || "(no stderr — likely timeout or malformed output)"}
\`\`\`

## Claim this was trying to verify
${claim}

## Your task

Produce a revised block that:
- Declares every variable it uses
- Ends with \`(check-sat)\`
- Uses only linear arithmetic over Int and Real (no forall, exists, arrays, String, Bool operators)
- Preserves the original semantics — fix the syntactic / type / timeout issue, do not change what it's trying to prove

Reply with ONLY the revised block in smt2 fences. No prose.
\`\`\`smt2
<revised>
(check-sat)
\`\`\``;

  let response;
  try {
    response = await provider.complete(prompt, {
      model,
      systemPrompt: "Fix broken SMT-LIB blocks. Reply with a single revised block in smt2 fences, nothing else.",
    });
  } catch {
    return null;
  }

  const m = response.text.match(/```(?:smt2|smt-lib|smtlib2)?\s*\n([\s\S]*?)```/i);
  if (!m) return null;
  const revised = m[1]!.trim();
  if (!revised.includes("(check-sat)")) return null;
  if (revised === smt2) return null;

  const { result } = verifyBlock(revised);
  return { smt2: revised, result };
}
