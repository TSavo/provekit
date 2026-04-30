import { Contract } from "../contracts";
import { Checker, CheckResult, extractPreconditions, extractDeclaredVars, namespaceBlock, runCheck } from "./Checker";

export class StrengtheningChecker implements Checker {
  readonly name = "strengthening";

  check(contracts: Contract[], callGraph: Map<string, string[]>): CheckResult[] {
    const results: CheckResult[] = [];
    const byFunction = new Map<string, Contract[]>();

    for (const c of contracts) {
      const fnKey = `${c.file}/${c.function}`;
      if (!byFunction.has(fnKey)) byFunction.set(fnKey, []);
      byFunction.get(fnKey)!.push(c);
    }

    const callers = new Map<string, Set<string>>();
    for (const [callerKey, callees] of callGraph) {
      for (const callee of callees) {
        if (!callers.has(callee)) callers.set(callee, new Set());
        callers.get(callee)!.add(callerKey);
      }
    }

    for (const c of contracts) {
      if (c.violations.length === 0) continue;

      const callerKeys = callers.get(c.function);
      if (!callerKeys || callerKeys.size === 0) continue;

      const allCallerPreconditions: string[][] = [];

      for (const callerKey of callerKeys) {
        const callerContracts = byFunction.get(callerKey);
        if (!callerContracts) continue;

        const thisCallerPre: string[] = [];
        for (const cc of callerContracts) {
          for (const p of cc.proven) {
            const pre = extractPreconditions(p.smt2);
            if (pre) thisCallerPre.push(...pre.preconditions);
          }
        }
        if (thisCallerPre.length > 0) {
          allCallerPreconditions.push(thisCallerPre);
        }
      }

      if (allCallerPreconditions.length < 2) continue;

      for (const violation of c.violations) {
        const violationLines = violation.smt2.split("\n");
        const violationDecls = violationLines.filter((l) =>
          l.trim().startsWith("(declare-const") || l.trim().startsWith("(define-fun")
        );
        const violationAsserts = violationLines.filter((l) => l.trim().startsWith("(assert"));
        if (violationAsserts.length === 0) continue;

        const intersected = allCallerPreconditions[0]!.filter((pre) =>
          allCallerPreconditions.every((callerPre) => callerPre.includes(pre))
        );

        if (intersected.length === 0) continue;

        const smt2 = `; STRENGTHENING: All ${callerKeys.size} callers of ${c.function} share these preconditions.
; Does the intersection eliminate the violation at ${c.function}:${c.line}?
; Shared caller preconditions (intersection):
${intersected.join("\n")}
; Violation block:
${violationDecls.join("\n")}
${violationAsserts.join("\n")}

(check-sat)
; unsat → ALL callers guard against this violation (strengthened contract)
; sat → violation survives even with shared caller guarantees`;

        results.push(runCheck(
          this.name,
          `${c.function}:${c.line} strengthened by ${callerKeys.size} callers: ${violation.claim.slice(0, 60)}`,
          `${c.function}:${c.line}`,
          smt2,
          "sat"
        ));
      }
    }

    return results;
  }
}
