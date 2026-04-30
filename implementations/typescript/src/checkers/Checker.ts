import { Contract } from "../contracts";
import { verifyBlock } from "../verifier";

export interface CheckResult {
  checker: string;
  description: string;
  sourceContract: string;
  targetContract?: string;
  smt2: string;
  expected: "sat" | "unsat";
  z3Result: "sat" | "unsat" | "unknown" | "error";
  verdict: "proven" | "violation" | "error";
  error?: string;
  witness?: string;
}

export interface Checker {
  readonly name: string;
  check(contracts: Contract[], callGraph: Map<string, string[]>): CheckResult[];
}

export function runCheck(
  checker: string,
  description: string,
  sourceContract: string,
  smt2: string,
  expected: "sat" | "unsat",
  targetContract?: string
): CheckResult {
  const { result, error, witness } = verifyBlock(smt2);
  let verdict: "proven" | "violation" | "error";
  if (expected === "unsat") {
    verdict = result === "unsat" ? "proven" : result === "sat" ? "violation" : "error";
  } else {
    verdict = result === "sat" ? "violation" : result === "unsat" ? "proven" : "error";
  }
  return { checker, description, sourceContract, targetContract, smt2, expected, z3Result: result, verdict, error, witness };
}

export function extractPreconditions(smt2: string): { decls: string[]; preconditions: string[] } | null {
  const lines = smt2.split("\n");
  const decls = lines.filter((l) =>
    l.trim().startsWith("(declare-const") || l.trim().startsWith("(define-fun")
  );
  const asserts = lines.filter((l) => l.trim().startsWith("(assert"));
  if (asserts.length < 2) return null;
  return { decls, preconditions: asserts.slice(0, -1) };
}

export function extractDeclaredVars(smt2: string): string[] {
  const names: string[] = [];
  for (const line of smt2.split("\n")) {
    const declMatch = line.match(/\(declare-const\s+(\S+)/);
    if (declMatch) names.push(declMatch[1]!);
    const defMatch = line.match(/\(define-fun\s+(\S+)/);
    if (defMatch) names.push(defMatch[1]!);
  }
  return names;
}

export function namespaceBlock(block: string, varNames: string[], prefix: string): string {
  let result = block;
  for (const name of varNames) {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    result = result.replace(new RegExp(`(?<!\\()${escaped}(?=\\s|\\))`, "g"), `${prefix}${name}`);
  }
  return result;
}
