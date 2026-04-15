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

  const codeBlockRegex = /```(?:smt2|smt-lib|smtlib2|scheme)?\s*\n([\s\S]*?)```/g;
  let match;

  while ((match = codeBlockRegex.exec(response)) !== null) {
    const content = match[1]!.trim();
    if (!content.includes("(check-sat)")) continue;

    const principleMatch = content.match(/;\s*PRINCIPLE:\s*(P\d+(?:\s*[,+&]\s*P\d+)*|\[NEW\])/i);
    const principle = principleMatch ? principleMatch[1]!.trim() : null;

    const firstCheckSat = content.indexOf("(check-sat)");
    const truncated = content.slice(0, firstCheckSat + "(check-sat)".length);
    blocks.push({ smt2: truncated, principle });
  }

  return blocks;
}

export function verifyBlock(smt2: string): { result: "sat" | "unsat" | "unknown" | "error"; error?: string } {
  const classify = (output: string): { result: "sat" | "unsat" | "unknown" | "error"; error?: string } => {
    const lines = output.trim().split("\n").map((l) => l.trim()).filter(Boolean);
    for (let i = lines.length - 1; i >= 0; i--) {
      if (lines[i] === "sat") return { result: "sat" };
      if (lines[i] === "unsat") return { result: "unsat" };
      if (lines[i] === "unknown") return { result: "unknown" };
    }
    return { result: "error", error: output.trim() || "empty output" };
  };

  try {
    const output = execSync("z3 -in -T:5", {
      input: smt2,
      encoding: "utf-8",
      timeout: 10000,
    });
    return classify(output);
  } catch (err: any) {
    const stderr = err.stderr?.toString() || "";
    const stdout = err.stdout?.toString() || "";
    if (stdout.trim()) return classify(stdout);
    if (stderr.includes("timeout")) return { result: "unknown", error: "Z3 timeout" };
    return { result: "error", error: stderr || err.message || "Z3 process failed" };
  }
}

export function isVacuous(smt2: string): boolean {
  const lines = smt2.split("\n").map((l) => l.trim());
  const declares = lines.filter((l) => l.startsWith("(declare-const"));
  const asserts = lines.filter((l) => l.startsWith("(assert"));
  const defineFuns = lines.filter((l) => l.startsWith("(define-fun"));

  if (declares.length === 0 && defineFuns.length === 0) return false;
  if (asserts.length === 0) return true;

  const declaredNames = new Set(
    declares.map((d) => {
      const m = d.match(/\(declare-const\s+(\S+)/);
      return m ? m[1]! : "";
    }).filter(Boolean)
  );

  for (const df of defineFuns) {
    const m = df.match(/\(define-fun\s+(\S+)/);
    if (m) declaredNames.add(m[1]!);
  }

  if (declaredNames.size < 2) return true;

  let transitionCount = 0;
  for (const a of asserts) {
    let referencedVars = 0;
    for (const name of declaredNames) {
      if (new RegExp(`\\b${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`).test(a)) referencedVars++;
    }
    if (referencedVars >= 2) transitionCount++;
  }

  return transitionCount === 0;
}

export function isTrivialIdentity(smt2: string): boolean {
  const lines = smt2.split("\n").map((l) => l.trim());
  const asserts = lines.filter((l) => l.startsWith("(assert"));

  if (asserts.length < 2) return false;

  const equalities: string[] = [];
  const negations: string[] = [];

  for (const a of asserts) {
    const eqMatch = a.match(/^\(assert\s+\(=\s+(\S+)\s+(\S+)\)\)$/);
    if (eqMatch) { equalities.push(`${eqMatch[1]} ${eqMatch[2]}`); continue; }
    const negMatch = a.match(/^\(assert\s+\(not\s+\(=\s+(\S+)\s+(\S+)\)\)\)$/);
    if (negMatch) negations.push(`${negMatch[1]} ${negMatch[2]}`);
  }

  for (const eq of equalities) {
    if (negations.includes(eq)) return true;
    const [a, b] = eq.split(" ");
    if (negations.includes(`${b} ${a}`)) return true;
  }

  return false;
}

export function verifyAll(response: string): VerificationResult[] {
  const blocks = extractSmt2Blocks(response);

  const vacuousCount = blocks.filter(({ smt2 }) => isVacuous(smt2)).length;
  const nonVacuous = blocks.filter(({ smt2 }) => !isVacuous(smt2));

  if (vacuousCount > 0) {
    console.log(`      (${vacuousCount} vacuous block${vacuousCount === 1 ? "" : "s"} filtered)`);
  }

  const results = nonVacuous.map(({ smt2, principle }) => {
    const { result, error } = verifyBlock(smt2);
    const trivial = isTrivialIdentity(smt2);
    return { smt2, z3Result: result, principle, error, trivial };
  });

  const trivialCount = results.filter((r) => r.trivial).length;
  if (trivialCount > 0) {
    console.log(`      (${trivialCount} trivial identity proof${trivialCount === 1 ? "" : "s"} flagged)`);
  }

  return results;
}
