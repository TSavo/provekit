import { execSync } from "child_process";

export interface VerificationResult {
  smt2: string;
  z3Result: "sat" | "unsat" | "unknown" | "error";
  principle: string | null;
  error?: string;
  trivial?: boolean;
  witness?: string;
  complexity: number;
  confidence?: "high" | "low";
}

export function proofComplexity(smt2: string): number {
  const lines = smt2.split("\n").map((l) => l.trim());
  const declares = new Set<string>();

  for (const line of lines) {
    const m = line.match(/\(declare-const\s+(\S+)/);
    if (m && m[1]) declares.add(m[1]);
    const dm = line.match(/\(define-fun\s+(\S+)/);
    if (dm && dm[1]) declares.add(dm[1]);
  }

  if (declares.size === 0) return 0;

  const asserts = lines.filter((l) => l.startsWith("(assert"));
  let transitions = 0;

  for (const a of asserts) {
    let refsFound = 0;
    for (const name of declares) {
      if (a.includes(name)) refsFound++;
    }
    if (refsFound >= 2) transitions++;
  }

  return transitions;
}

export function extractSmt2Blocks(response: string): { smt2: string; principle: string | null }[] {
  const blocks: { smt2: string; principle: string | null }[] = [];

  const codeBlockRegex = /```(?:smt2|smt-lib|smtlib2|scheme)?\s*\n([\s\S]*?)```/g;
  let match;

  while ((match = codeBlockRegex.exec(response)) !== null) {
    const content = match[1];
    if (!content || !content.includes("(check-sat)")) continue;
    const trimmed = content.trim();

    const principleMatch = trimmed.match(/;\s*PRINCIPLE:\s*([a-zA-Z0-9_-]+(?:\s*[,+&]\s*[a-zA-Z0-9_-]+)*|\[NEW\])/i);
    const principle = principleMatch ? principleMatch[1]!.trim() : null;

    const firstCheckSat = trimmed.indexOf("(check-sat)");
    const truncated = trimmed.slice(0, firstCheckSat + "(check-sat)".length);
    blocks.push({ smt2: truncated, principle });
  }

  return blocks;
}

export function verifyBlock(smt2: string): { result: "sat" | "unsat" | "unknown" | "error"; error?: string; witness?: string } {
  const classify = (output: string): { result: "sat" | "unsat" | "unknown" | "error"; error?: string } => {
    const lines = output.trim().split("\n").map((l) => l.trim()).filter(Boolean);
    for (let i = lines.length - 1; i >= 0; i--) {
      const line = lines[i];
      if (line === "sat") return { result: "sat" };
      if (line === "unsat") return { result: "unsat" };
      if (line === "unknown") return { result: "unknown" };
    }
    return { result: "error", error: output.trim() || "empty output" };
  };

  try {
    const output = execSync("z3 -in -T:5", {
      input: smt2,
      encoding: "utf-8",
      timeout: 6000,
    });
    const verdict = classify(output);
    if (verdict.result === "sat") {
      return { ...verdict, witness: extractWitness(smt2) };
    }
    return verdict;
  } catch (err: any) {
    const stderr = err?.stderr?.toString() || "";
    const stdout = err?.stdout?.toString() || "";
    if (stdout.trim()) {
      const verdict = classify(stdout);
      if (verdict.result === "sat") {
        return { ...verdict, witness: extractWitness(smt2) };
      }
      return verdict;
    }
    if (stderr.includes("timeout")) return { result: "unknown", error: "Z3 timeout" };
    return { result: "error", error: stderr || err?.message || "Z3 process failed" };
  }
}

function extractWitness(smt2: string): string | undefined {
  const withModel = smt2.replace("(check-sat)", "(check-sat)\n(get-model)");
  try {
    const output = execSync("z3 -in -T:5", {
      input: withModel,
      encoding: "utf-8",
      timeout: 6000,
    });
    const modelMatch = output.match(/\(model[\s\S]*?\n\)|\(\s*\n\s+\(define-fun[\s\S]*?\n\)/);
    if (modelMatch) return modelMatch[0];
    const lines = output.trim().split("\n");
    const satIdx = lines.findIndex((l) => l.trim() === "sat");
    if (satIdx >= 0 && satIdx + 1 < lines.length) {
      return lines.slice(satIdx + 1).join("\n");
    }
    return undefined;
  } catch (err: any) {
    console.log(`[verifier] extractWitness failed: ${err?.message?.slice(0, 60) || "unknown error"}`);
    return undefined;
  }
}

export function isVacuous(smt2: string): boolean {
  const lines = smt2.split("\n").map((l) => l.trim());
  const declares = lines.filter((l) => l.startsWith("(declare-const"));
  const asserts = lines.filter((l) => l.startsWith("(assert"));
  const defineFuns = lines.filter((l) => l.startsWith("(define-fun"));

  if (declares.length === 0 && defineFuns.length === 0) return false;
  if (asserts.length === 0) return true;

  const declaredNames = new Set<string>();
  for (const d of declares) {
    const m = d.match(/\(declare-const\s+(\S+)/);
    if (m && m[1]) declaredNames.add(m[1]);
  }
  for (const df of defineFuns) {
    const m = df.match(/\(define-fun\s+(\S+)/);
    if (m && m[1]) declaredNames.add(m[1]);
  }

  if (declaredNames.size < 2) return true;

  let transitionCount = 0;
  for (const a of asserts) {
    let referencedVars = 0;
    for (const name of declaredNames) {
      const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      if (new RegExp(`\\b${escaped}\\b`).test(a)) referencedVars++;
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
    if (eqMatch && eqMatch[1] && eqMatch[2]) {
      equalities.push(`${eqMatch[1]} ${eqMatch[2]}`);
      continue;
    }
    const negMatch = a.match(/^\(assert\s+\(not\s+\(=\s+(\S+)\s+(\S+)\)\)\)$/);
    if (negMatch && negMatch[1] && negMatch[2]) {
      negations.push(`${negMatch[1]} ${negMatch[2]}`);
    }
  }

  for (const eq of equalities) {
    if (negations.includes(eq)) return true;
    const parts = eq.split(" ");
    if (parts.length === 2 && negations.includes(`${parts[1]} ${parts[0]}`)) return true;
  }

  return false;
}

export function verifyAll(response: string): VerificationResult[] {
  const blocks = extractSmt2Blocks(response);

  if (blocks.length === 0) {
    console.log(`      (no SMT-LIB blocks found in response)`);
    return [];
  }

  const vacuousCount = blocks.filter(({ smt2 }) => isVacuous(smt2)).length;
  const nonVacuous = blocks.filter(({ smt2 }) => !isVacuous(smt2));

  if (vacuousCount > 0) {
    console.log(`      (${vacuousCount} vacuous block${vacuousCount === 1 ? "" : "s"} filtered)`);
  }

  if (nonVacuous.length === 0 && vacuousCount > 0) {
    console.log(`      WARNING: all ${blocks.length} blocks were vacuous — no meaningful proofs`);
    return [];
  }

  const results: VerificationResult[] = [];
  for (const { smt2, principle } of nonVacuous) {
    const { result, error, witness } = verifyBlock(smt2);
    const trivial = isTrivialIdentity(smt2);
    if (!trivial) {
      results.push({ smt2, z3Result: result, principle, error, witness, complexity: proofComplexity(smt2) });
    }
  }

  const trivialCount = nonVacuous.length - results.length;
  if (trivialCount > 0) {
    console.log(`      (${trivialCount} trivial identity proof${trivialCount === 1 ? "" : "s"} excluded)`);
  }

  return results;
}
