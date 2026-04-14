import { execSync } from "child_process";

export interface VerificationResult {
  smt2: string;
  z3Result: "sat" | "unsat" | "unknown" | "error";
  principle: string | null;
  error?: string;
  trivial?: boolean;
}

export function extractSmt2Blocks(response: string): { smt2: string; principle: string | null }[] {
  const blocks: { smt2: string; principle: string | null }[] = [];

  // Match ```smt2 or ```smt-lib or ```smtlib2 or ``` followed by SMT content
  const codeBlockRegex = /```(?:smt2|smt-lib|smtlib2|scheme)?\s*\n([\s\S]*?)```/g;
  let match;

  while ((match = codeBlockRegex.exec(response)) !== null) {
    const content = match[1]!.trim();
    // Only include blocks that have (check-sat)
    if (content.includes("(check-sat)")) {
      // Extract principle tag from comments
      const principleMatch = content.match(/;\s*PRINCIPLE:\s*(P\d+(?:\s*[,+&]\s*P\d+)*|\[NEW\])/i);
      const principle = principleMatch ? principleMatch[1]!.trim() : null;

      blocks.push({ smt2: content, principle });
    }
  }

  return blocks;
}

export function verifyBlock(smt2: string): { result: "sat" | "unsat" | "unknown" | "error"; error?: string } {
  try {
    const output = execSync("z3 -in -T:5", {
      input: smt2,
      encoding: "utf-8",
      timeout: 10000,
    }).trim();

    if (output === "sat") return { result: "sat" };
    if (output === "unsat") return { result: "unsat" };
    if (output === "unknown") return { result: "unknown" };
    return { result: "error", error: output };
  } catch (err: any) {
    const stderr = err.stderr?.toString() || "";
    const stdout = err.stdout?.toString()?.trim() || "";
    if (stdout === "sat") return { result: "sat" };
    if (stdout === "unsat") return { result: "unsat" };
    return { result: "error", error: stderr || stdout || err.message };
  }
}

/**
 * Detect vacuous SMT-LIB blocks — those that are trivially satisfiable
 * because they assert a condition on an unconstrained variable with no
 * code-model transitions.
 *
 * A vacuous block has:
 * - declare-const variables
 * - assertion(s) that are ONLY the violation condition
 * - NO transitional assertions (= new_x (- old_x quantity)), etc.
 *
 * The heuristic: if the only non-comment, non-declare, non-check-sat
 * assertions are simple comparisons on single variables (< x 0), (= x 0),
 * (> x CONST) with no binary operations referencing other declared vars,
 * the block is vacuous.
 */
export function isVacuous(smt2: string): boolean {
  const lines = smt2.split("\n").map((l) => l.trim());
  const declares = lines.filter((l) => l.startsWith("(declare-const"));
  const asserts = lines.filter((l) => l.startsWith("(assert"));

  if (declares.length === 0 || asserts.length === 0) return false;

  const declaredNames = new Set(
    declares.map((d) => {
      const m = d.match(/\(declare-const\s+(\S+)/);
      return m ? m[1]! : "";
    }).filter(Boolean)
  );

  // Count assertions that reference multiple declared variables (transitions)
  let transitionCount = 0;
  for (const a of asserts) {
    let referencedVars = 0;
    for (const name of declaredNames) {
      if (new RegExp(`\\b${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`).test(a)) referencedVars++;
    }
    // A transition references at least 2 declared variables (e.g., new_x = old_x - quantity)
    if (referencedVars >= 2) transitionCount++;
  }

  // If no assertions reference multiple variables, this is likely vacuous
  return transitionCount === 0;
}

/**
 * Detect trivial identity proofs — blocks that assert (= x y) and then
 * assert (not (= x y)). These are tautologically unsat and prove nothing
 * beyond the identity of an assignment.
 */
export function isTrivialIdentity(smt2: string): boolean {
  const lines = smt2.split("\n").map((l) => l.trim());
  const asserts = lines.filter((l) => l.startsWith("(assert"));

  if (asserts.length !== 2) return false;

  // Pattern: one assert is (= A B), the other is (not (= A B))
  const hasEquality = asserts.some((a) => /^\(assert\s+\(=\s+\S+\s+\S+\)\)$/.test(a));
  const hasNegation = asserts.some((a) => /^\(assert\s+\(not\s+\(=\s+\S+\s+\S+\)\)\)$/.test(a));

  return hasEquality && hasNegation;
}

export function verifyAll(response: string): VerificationResult[] {
  const blocks = extractSmt2Blocks(response);
  const vacuousCount = blocks.filter(({ smt2 }) => isVacuous(smt2)).length;
  if (vacuousCount > 0) {
    console.log(`      (${vacuousCount} vacuous block${vacuousCount === 1 ? "" : "s"} filtered)`);
  }
  return blocks
    .filter(({ smt2 }) => !isVacuous(smt2))
    .map(({ smt2, principle }) => {
      const { result, error } = verifyBlock(smt2);
      const trivial = isTrivialIdentity(smt2);
      return { smt2, z3Result: result, principle, error, trivial };
    });
}
