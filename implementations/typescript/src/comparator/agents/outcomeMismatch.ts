import type { GapReport } from "../core.js";

export interface OutcomeMismatchInput {
  smtOutcome: { kind: "returned" | "threw" };
  runtimeOutcome: { kind: "returned" | "threw" | "untestable"; error?: string };
  smtConstant: string;
}

export function outcomeMismatchAgent(input: OutcomeMismatchInput): GapReport | null {
  const { smtOutcome, runtimeOutcome, smtConstant } = input;
  if (runtimeOutcome.kind === "untestable") return null;
  if (smtOutcome.kind === runtimeOutcome.kind) return null;

  const smtVerb = smtOutcome.kind === "threw" ? "throw" : "return";
  const rtVerb = runtimeOutcome.kind === "threw" ? "threw" : "returned";
  const rtDetail = runtimeOutcome.kind === "threw" ? `: ${runtimeOutcome.error}` : " a value";
  return {
    kind: "outcome_mismatch",
    smtConstant,
    explanation: `SMT modeled the function as ${smtVerb}; runtime ${rtVerb}${rtDetail}. The encoding does not account for the runtime's actual control-flow outcome.`,
  };
}
