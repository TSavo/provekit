import { CallSiteContext } from "./ContextPhase";

export function buildSignalFrame(signals: CallSiteContext[]): string {
  const lines = signals.map((s, i) => {
    const typeHint = s.signalType === "log" ? `logs: \`${s.signalText.slice(0, 100)}\``
      : s.signalType === "comment" ? `comment: "${s.signalText.slice(0, 100)}"`
      : s.signalType.startsWith("name:") ? `function name \`${s.functionName}\` promises: ${s.signalText.slice(0, 100)}`
      : `signal: \`${s.signalText.slice(0, 100)}\``;
    return `  ${i + 1}. Line ${s.line} [${s.signalType}]: ${typeHint}`;
  }).join("\n");

  const calleesSet = new Set<string>();
  for (const s of signals) {
    for (const c of s.callees) calleesSet.add(c);
  }

  const calledBySet = new Set<string>();
  for (const s of signals) {
    for (const c of s.calledBy) calledBySet.add(c);
  }

  let callGraph = "";
  if (calleesSet.size > 0) {
    callGraph += `\nThis function calls: ${[...calleesSet].join(", ")}`;
  }
  if (calledBySet.size > 0) {
    callGraph += `\nThis function is called by: ${[...calledBySet].join(", ")}`;
  }

  return `This function has ${signals.length} verification point${signals.length === 1 ? "" : "s"}:
${lines}
${callGraph}

Every log statement is a claim about program state. Every comment is programmer intent. Every function name is a contract. For EACH verification point, derive formal invariants. Tag each SMT-LIB block with the line number it verifies.`;
}
