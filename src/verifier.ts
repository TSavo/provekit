import { execSync } from "child_process";

export interface VerificationResult {
  smt2: string;
  z3Result: "sat" | "unsat" | "unknown" | "error";
  principle: string | null;
  error?: string;
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
    const output = execSync(`echo '${smt2.replace(/'/g, "'\\''")}' | z3 -in -T:5`, {
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

export function verifyAll(response: string): VerificationResult[] {
  const blocks = extractSmt2Blocks(response);
  return blocks.map(({ smt2, principle }) => {
    const { result, error } = verifyBlock(smt2);
    return { smt2, z3Result: result, principle, error };
  });
}
