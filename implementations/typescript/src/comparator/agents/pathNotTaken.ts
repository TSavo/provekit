import type { GapReport } from "../core.js";

export interface PathNotTakenInput {
  signalLine: number;
  visitedLines: Set<number>;
  smtConstant: string;
}

export function pathNotTakenAgent(input: PathNotTakenInput): GapReport | null {
  const { signalLine, visitedLines, smtConstant } = input;
  if (visitedLines.size === 0) return null;
  if (visitedLines.has(signalLine)) return null;

  return {
    kind: "path_not_taken",
    smtConstant,
    explanation: `SMT witness claims a value at line ${signalLine}, but runtime did not reach that line. The witness inputs drive execution down a different path than the encoding assumed.`,
  };
}
