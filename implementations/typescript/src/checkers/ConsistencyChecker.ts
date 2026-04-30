import { Contract } from "../contracts";
import { verifyBlock } from "../verifier";
import { Checker, CheckResult, extractPreconditions, extractDeclaredVars, namespaceBlock } from "./Checker";

export class ConsistencyChecker implements Checker {
  readonly name = "consistency";

  check(contracts: Contract[]): CheckResult[] {
    const namespacedBlocks: string[] = [];
    let proofIndex = 0;

    for (const c of contracts) {
      for (const p of c.proven) {
        const prefix = `c${proofIndex}_`;
        proofIndex++;

        const extracted = extractPreconditions(p.smt2);
        if (!extracted) continue;

        const selfCheck = [...extracted.decls, ...extracted.preconditions, "(check-sat)"].join("\n");
        const selfResult = verifyBlock(selfCheck);
        if (selfResult.result === "unsat") continue;

        const varNames = extractDeclaredVars(p.smt2);
        const block = namespaceBlock(
          [...extracted.decls, ...extracted.preconditions].join("\n"),
          varNames,
          prefix
        );
        namespacedBlocks.push(block);
      }
    }

    if (namespacedBlocks.length < 2) return [];

    const smt2 = `; CONSISTENCY: Are preconditions across all contracts mutually satisfiable?
${namespacedBlocks.join("\n\n")}

(check-sat)`;

    const { result, error, witness } = verifyBlock(smt2);
    const verdict: "proven" | "violation" | "error" =
      result === "sat" ? "proven" : result === "unsat" ? "violation" : "error";

    return [{
      checker: this.name,
      description: `Cross-contract consistency: ${contracts.length} contracts, ${namespacedBlocks.length} precondition sets`,
      sourceContract: "all",
      smt2,
      expected: "sat",
      z3Result: result,
      verdict,
      error,
      witness,
    }];
  }
}
