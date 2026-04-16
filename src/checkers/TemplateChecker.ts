import { Contract } from "../contracts";
import { Checker, CheckResult, runCheck, extractDeclaredVars } from "./Checker";

export class TemplateChecker implements Checker {
  readonly name = "template";

  check(contracts: Contract[]): CheckResult[] {
    const results: CheckResult[] = [];

    for (const contract of contracts) {
      for (const violation of contract.violations) {
        if (!violation.principle) continue;
        const tag = violation.principle.replace(/[\[\]]/g, "").trim();
        if (tag === "NEW") continue;

        results.push(runCheck(
          this.name,
          `[${tag}] ${contract.function}:${contract.line}: ${violation.claim.slice(0, 60)}`,
          `${contract.function}:${contract.line}`,
          violation.smt2,
          "sat"
        ));
      }
    }

    for (const contract of contracts) {
      for (const proven of contract.proven) {
        const vars = extractDeclaredVars(proven.smt2);
        if (vars.length === 0) continue;

        for (const caller of contracts) {
          if (caller.file !== contract.file) continue;
          if (caller.line >= contract.line) continue;

          results.push(runCheck(
            this.name,
            `Precondition propagation: ${caller.function}:${caller.line} → ${contract.function}:${contract.line}: ${proven.claim.slice(0, 60)}`,
            `${contract.function}:${contract.line}`,
            proven.smt2,
            "unsat",
            `${caller.function}:${caller.line}`
          ));
        }
      }
    }

    results.push(...this.checkStateMutation(contracts));
    return results;
  }

  private checkStateMutation(contracts: Contract[]): CheckResult[] {
    const results: CheckResult[] = [];
    const byFunction = new Map<string, Contract[]>();

    for (const c of contracts) {
      const key = `${c.file}:${c.function}`;
      if (!byFunction.has(key)) byFunction.set(key, []);
      byFunction.get(key)!.push(c);
    }

    for (const [, fnContracts] of byFunction) {
      if (fnContracts.length < 2) continue;
      const sorted = [...fnContracts].sort((a, b) => a.line - b.line);

      for (let i = 0; i < sorted.length - 1; i++) {
        const earlier = sorted[i]!;
        const later = sorted[i + 1]!;

        for (const prop of earlier.proven) {
          const vars = extractDeclaredVars(prop.smt2);
          if (vars.length === 0) continue;

          results.push(runCheck(
            this.name,
            `State mutation: ${earlier.function}:${earlier.line} → ${later.function}:${later.line}: ${prop.claim.slice(0, 60)}`,
            `${earlier.function}:${earlier.line}`,
            prop.smt2,
            "unsat",
            `${later.function}:${later.line}`
          ));
        }
      }
    }

    return results;
  }
}
