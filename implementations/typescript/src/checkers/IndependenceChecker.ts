import { Contract } from "../contracts";
import { verifyBlock } from "../verifier";
import { Checker, CheckResult, extractPreconditions, extractDeclaredVars, namespaceBlock } from "./Checker";

export class IndependenceChecker implements Checker {
  readonly name = "independence";

  check(contracts: Contract[]): CheckResult[] {
    const results: CheckResult[] = [];

    const provenBlocks: { contract: Contract; prop: { claim: string; smt2: string }; index: number }[] = [];
    for (const c of contracts) {
      for (let i = 0; i < c.proven.length; i++) {
        provenBlocks.push({ contract: c, prop: c.proven[i]!, index: provenBlocks.length });
      }
    }

    if (provenBlocks.length < 3) return [];

    const baseBlocks: { decls: string; preconditions: string; vars: string[]; prefix: string }[] = [];
    for (const pb of provenBlocks) {
      const extracted = extractPreconditions(pb.prop.smt2);
      if (!extracted) {
        baseBlocks.push({ decls: "", preconditions: "", vars: [], prefix: `c${pb.index}_` });
        continue;
      }
      const vars = extractDeclaredVars(pb.prop.smt2);
      const prefix = `c${pb.index}_`;
      const block = namespaceBlock(
        [...extracted.decls, ...extracted.preconditions].join("\n"),
        vars, prefix
      );
      const declPart = extracted.decls.map((d) => {
        let r = d;
        for (const v of vars) {
          const escaped = v.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
          r = r.replace(new RegExp(`(?<!\\()${escaped}(?=\\s|\\))`, "g"), `${prefix}${v}`);
        }
        return r;
      }).join("\n");
      const prePart = extracted.preconditions.map((p) => {
        let r = p;
        for (const v of vars) {
          const escaped = v.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
          r = r.replace(new RegExp(`(?<!\\()${escaped}(?=\\s|\\))`, "g"), `${prefix}${v}`);
        }
        return r;
      }).join("\n");
      baseBlocks.push({ decls: declPart, preconditions: prePart, vars, prefix });
    }

    const validBlocks = baseBlocks.filter((b) => b.preconditions.length > 0);
    if (validBlocks.length < 3) return [];

    const fullSet = validBlocks.map((b) => `${b.decls}\n${b.preconditions}`).join("\n\n");
    const fullSmt2 = `${fullSet}\n(check-sat)`;
    const fullResult = verifyBlock(fullSmt2);
    if (fullResult.result !== "sat") return [];

    for (let i = 0; i < validBlocks.length; i++) {
      const without = validBlocks
        .filter((_, j) => j !== i)
        .map((b) => `${b.decls}\n${b.preconditions}`)
        .join("\n\n");

      const smt2 = `; INDEPENDENCE: Is proof #${i} load-bearing?
; Removing ${provenBlocks[i]?.contract.function}:${provenBlocks[i]?.contract.line} — ${provenBlocks[i]?.prop.claim.slice(0, 60)}
${without}

(check-sat)
; unsat → removing this proof made the set inconsistent — this proof is LOAD-BEARING
; sat → proof can be removed without breaking consistency`;

      const { result, error, witness } = verifyBlock(smt2);

      if (result === "unsat") {
        results.push({
          checker: this.name,
          description: `LOAD-BEARING: ${provenBlocks[i]?.contract.function}:${provenBlocks[i]?.contract.line} — ${provenBlocks[i]?.prop.claim.slice(0, 60)}`,
          sourceContract: `${provenBlocks[i]?.contract.function}:${provenBlocks[i]?.contract.line}`,
          smt2,
          expected: "sat",
          z3Result: result,
          verdict: "violation",
          error,
          witness,
        });
      }
    }

    return results;
  }
}
