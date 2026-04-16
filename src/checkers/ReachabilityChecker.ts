import { Contract } from "../contracts";
import { Checker, CheckResult, extractPreconditions, extractDeclaredVars, namespaceBlock, runCheck } from "./Checker";

export class ReachabilityChecker implements Checker {
  readonly name = "reachability";

  check(contracts: Contract[], callGraph: Map<string, string[]>): CheckResult[] {
    const results: CheckResult[] = [];
    const byFunction = new Map<string, Contract[]>();

    for (const c of contracts) {
      const fnKey = `${c.file}/${c.function}`;
      if (!byFunction.has(fnKey)) byFunction.set(fnKey, []);
      byFunction.get(fnKey)!.push(c);
    }

    const callers = new Map<string, string[]>();
    for (const [callerKey, callees] of callGraph) {
      for (const callee of callees) {
        if (!callers.has(callee)) callers.set(callee, []);
        callers.get(callee)!.push(callerKey);
      }
    }

    for (const c of contracts) {
      if (c.violations.length === 0) continue;

      const fnKey = `${c.file}/${c.function}`;
      const callerKeys = callers.get(c.function) || [];
      if (callerKeys.length === 0) continue;

      for (const violation of c.violations) {
        const violationLines = violation.smt2.split("\n");
        const violationDecls = violationLines.filter((l) =>
          l.trim().startsWith("(declare-const") || l.trim().startsWith("(define-fun")
        );
        const violationAsserts = violationLines.filter((l) => l.trim().startsWith("(assert"));
        if (violationAsserts.length === 0) continue;

        for (const callerKey of callerKeys) {
          const callerContracts = byFunction.get(callerKey);
          if (!callerContracts) continue;

          const callerPreconditions: string[] = [];
          const callerDecls: string[] = [];

          for (const cc of callerContracts) {
            for (const p of cc.proven) {
              const pre = extractPreconditions(p.smt2);
              if (!pre) continue;
              const vars = extractDeclaredVars(p.smt2);
              const ns = namespaceBlock(
                [...pre.decls, ...pre.preconditions].join("\n"),
                vars, `guard_${callerContracts.indexOf(cc)}_`
              );
              callerPreconditions.push(ns);
            }
          }

          if (callerPreconditions.length === 0) continue;

          const smt2 = `; REACHABILITY: Is ${c.function}:${c.line} violation reachable through ${callerKey}?
; Caller guards:
${callerPreconditions.join("\n")}
; Violation block:
${violationDecls.join("\n")}
${violationAsserts.join("\n")}

(check-sat)
; unsat → violation is UNREACHABLE through this caller (guards prevent it)
; sat → violation is still reachable even with caller guards`;

          results.push(runCheck(
            this.name,
            `${c.function}:${c.line} via ${callerKey}: ${violation.claim.slice(0, 60)}`,
            `${c.function}:${c.line}`,
            smt2,
            "sat",
            callerKey
          ));
        }
      }
    }

    return results;
  }
}
