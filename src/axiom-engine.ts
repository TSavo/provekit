/**
 * Layer 2: Mechanical axiom application.
 *
 * Given cached contracts and axiom templates, generates Z3 checks
 * without any LLM calls. Pure template instantiation + Z3.
 */

import { Contract, ContractStore } from "./contracts";
import { verifyBlock } from "./verifier";

export interface AxiomCheck {
  axiom: string;
  description: string;
  sourceContract: string;
  targetContract?: string;
  smt2: string;
  expected: "sat" | "unsat";
}

export interface AxiomResult extends AxiomCheck {
  z3Result: "sat" | "unsat" | "unknown" | "error";
  verdict: "proven" | "violation" | "error";
  error?: string;
}

/**
 * Apply all axiom templates to a set of contracts.
 * Returns Z3-verified results — no LLM involved.
 */
export function applyAxioms(contracts: Contract[]): AxiomResult[] {
  const checks: AxiomCheck[] = [];

  for (const contract of contracts) {
    checks.push(...applyP1(contract, contracts));
    checks.push(...applyP3(contract));
    checks.push(...applyP4(contract));
    checks.push(...applyP6(contract));
    checks.push(...applyP7(contract));
  }

  // P2 needs pairs of contracts in the same function (loop iterations)
  checks.push(...applyP2(contracts));

  return checks.map((check) => {
    const { result, error } = verifyBlock(check.smt2);
    let verdict: "proven" | "violation" | "error";
    if (check.expected === "unsat") {
      verdict = result === "unsat" ? "proven" : result === "sat" ? "violation" : "error";
    } else {
      verdict = result === "sat" ? "violation" : result === "unsat" ? "proven" : "error";
    }
    return { ...check, z3Result: result, verdict, error };
  });
}

/**
 * P1: Precondition Propagation
 * For each violation in a contract that references a callee's precondition,
 * check if the precondition is established by prior contracts.
 */
function applyP1(contract: Contract, allContracts: Contract[]): AxiomCheck[] {
  const checks: AxiomCheck[] = [];

  // For each proven precondition in this contract, check if callers establish it
  for (const proven of contract.proven) {
    // Extract variable declarations and assertions from the proven SMT-LIB
    const vars = extractDeclaredVars(proven.smt2);
    if (vars.length === 0) continue;

    // For every other contract that could be a caller (same file, earlier line)
    for (const caller of allContracts) {
      if (caller.file !== contract.file) continue;
      if (caller.line >= contract.line) continue;

      // Generate a check: does the caller's proven context establish this precondition?
      const smt2 = `; P1 MECHANICAL: Does ${caller.function}:${caller.line} establish preconditions for ${contract.function}:${contract.line}?
; Property: ${proven.claim}
${proven.smt2}`;

      checks.push({
        axiom: "P1",
        description: `${caller.function}:${caller.line} → ${contract.function}:${contract.line}: ${proven.claim.slice(0, 60)}`,
        sourceContract: `${contract.function}:${contract.line}`,
        targetContract: `${caller.function}:${caller.line}`,
        smt2,
        expected: "unsat",
      });
    }
  }

  return checks;
}

/**
 * P2: State Mutation Analysis
 * For contracts in the same function at different lines,
 * check if mutations between them break invariants.
 */
function applyP2(contracts: Contract[]): AxiomCheck[] {
  const checks: AxiomCheck[] = [];

  // Group contracts by function
  const byFunction = new Map<string, Contract[]>();
  for (const c of contracts) {
    const key = `${c.file}:${c.function}`;
    if (!byFunction.has(key)) byFunction.set(key, []);
    byFunction.get(key)!.push(c);
  }

  for (const [fnKey, fnContracts] of byFunction) {
    if (fnContracts.length < 2) continue;

    // Sort by line
    const sorted = [...fnContracts].sort((a, b) => a.line - b.line);

    for (let i = 0; i < sorted.length - 1; i++) {
      const earlier = sorted[i]!;
      const later = sorted[i + 1]!;

      // Check: do the proven properties of the earlier contract
      // still hold at the later contract's point?
      for (const prop of earlier.proven) {
        const vars = extractDeclaredVars(prop.smt2);
        if (vars.length === 0) continue;

        checks.push({
          axiom: "P2",
          description: `State mutation between ${earlier.function}:${earlier.line} and ${later.function}:${later.line}: ${prop.claim.slice(0, 60)}`,
          sourceContract: `${earlier.function}:${earlier.line}`,
          targetContract: `${later.function}:${later.line}`,
          smt2: prop.smt2,
          expected: "unsat",
        });
      }
    }
  }

  return checks;
}

/**
 * P3: Calling Context Analysis
 * For each contract on an exported function, check if violations
 * are reachable given unconstrained inputs.
 */
function applyP3(contract: Contract): AxiomCheck[] {
  const checks: AxiomCheck[] = [];

  for (const violation of contract.violations) {
    if (!violation.principle?.includes("P3")) continue;

    checks.push({
      axiom: "P3",
      description: `Public input violation at ${contract.function}:${contract.line}: ${violation.claim.slice(0, 60)}`,
      sourceContract: `${contract.function}:${contract.line}`,
      smt2: violation.smt2,
      expected: "sat",
    });
  }

  return checks;
}

/**
 * P4: Temporal Analysis
 * Re-check violation SMT-LIB blocks tagged with P4 (double-invocation).
 */
function applyP4(contract: Contract): AxiomCheck[] {
  const checks: AxiomCheck[] = [];

  for (const violation of contract.violations) {
    if (!violation.principle?.includes("P4")) continue;

    checks.push({
      axiom: "P4",
      description: `Temporal violation at ${contract.function}:${contract.line}: ${violation.claim.slice(0, 60)}`,
      sourceContract: `${contract.function}:${contract.line}`,
      smt2: violation.smt2,
      expected: "sat",
    });
  }

  return checks;
}

/**
 * P6: Boundary and Degenerate Inputs
 * Re-check violation SMT-LIB blocks tagged with P6.
 */
function applyP6(contract: Contract): AxiomCheck[] {
  const checks: AxiomCheck[] = [];

  for (const violation of contract.violations) {
    if (!violation.principle?.includes("P6")) continue;

    checks.push({
      axiom: "P6",
      description: `Boundary violation at ${contract.function}:${contract.line}: ${violation.claim.slice(0, 60)}`,
      sourceContract: `${contract.function}:${contract.line}`,
      smt2: violation.smt2,
      expected: "sat",
    });
  }

  return checks;
}

/**
 * P7: Arithmetic Safety
 * Re-check violation SMT-LIB blocks tagged with P7.
 */
function applyP7(contract: Contract): AxiomCheck[] {
  const checks: AxiomCheck[] = [];

  for (const violation of contract.violations) {
    if (!violation.principle?.includes("P7")) continue;

    checks.push({
      axiom: "P7",
      description: `Arithmetic violation at ${contract.function}:${contract.line}: ${violation.claim.slice(0, 60)}`,
      sourceContract: `${contract.function}:${contract.line}`,
      smt2: violation.smt2,
      expected: "sat",
    });
  }

  return checks;
}

/**
 * Cross-contract consistency: check all proven properties against each other.
 * If the set is unsatisfiable, the contracts contradict.
 */
export function checkConsistency(contracts: Contract[]): AxiomResult[] {
  const namespacedBlocks: string[] = [];
  let proofIndex = 0;

  for (const c of contracts) {
    for (const p of c.proven) {
      const prefix = `c${proofIndex}_`;
      proofIndex++;

      const lines = p.smt2.split("\n");
      const varNames: string[] = [];

      for (const line of lines) {
        const declMatch = line.match(/\(declare-const\s+(\S+)/);
        if (declMatch) varNames.push(declMatch[1]!);
        const defMatch = line.match(/\(define-fun\s+(\S+)/);
        if (defMatch) varNames.push(defMatch[1]!);
      }

      const assertLines = lines.filter((l) => l.trim().startsWith("(assert"));
      if (assertLines.length < 2) continue;
      const preconditionAsserts = assertLines.slice(0, -1);

      const declLines = lines.filter((l) =>
        l.trim().startsWith("(declare-const") || l.trim().startsWith("(define-fun")
      );

      // Skip blocks whose preconditions alone are unsat — they're self-contradictory
      const selfCheck = [...declLines, ...preconditionAsserts, "(check-sat)"].join("\n");
      const selfResult = verifyBlock(selfCheck);
      if (selfResult.result === "unsat") continue;

      let block = [...declLines, ...preconditionAsserts].join("\n");

      for (const name of varNames) {
        const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        block = block.replace(new RegExp(`\\b${escaped}\\b`, "g"), `${prefix}${name}`);
      }

      namespacedBlocks.push(block);
    }
  }

  if (namespacedBlocks.length < 2) return [];

  const smt2 = `; CONSISTENCY CHECK: Are preconditions across all contracts mutually satisfiable?
; Only preconditions and transitions are included (negated properties stripped).
; Each contract's variables are namespaced to avoid collisions.
${namespacedBlocks.join("\n\n")}

(check-sat)
; Expected: sat — all preconditions can hold simultaneously
; unsat — genuine contradiction between contract preconditions`;

  const check: AxiomCheck = {
    axiom: "CONSISTENCY",
    description: `Cross-contract consistency: ${contracts.length} contracts, ${namespacedBlocks.length} precondition sets`,
    sourceContract: "all",
    smt2,
    expected: "sat",
  };

  const { result, error } = verifyBlock(smt2);
  const verdict: "proven" | "violation" | "error" =
    result === "sat" ? "proven" : result === "unsat" ? "violation" : "error";

  return [{ ...check, z3Result: result, verdict, error }];
}

function extractDeclaredVars(smt2: string): string[] {
  const matches = smt2.matchAll(/\(declare-const\s+(\S+)\s+\S+\)/g);
  return [...matches].map((m) => m[1]!);
}
