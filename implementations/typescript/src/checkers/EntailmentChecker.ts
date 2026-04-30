import { Contract } from "../contracts";
import { Checker, CheckResult, extractPreconditions, extractDeclaredVars, namespaceBlock, runCheck } from "./Checker";

export class EntailmentChecker implements Checker {
  readonly name = "entailment";

  check(contracts: Contract[], callGraph: Map<string, string[]>): CheckResult[] {
    const results: CheckResult[] = [];
    const byFunction = new Map<string, Contract[]>();

    for (const c of contracts) {
      const fnKey = `${c.file}/${c.function}`;
      if (!byFunction.has(fnKey)) byFunction.set(fnKey, []);
      byFunction.get(fnKey)!.push(c);
    }

    for (const [callerFnKey, callees] of callGraph) {
      const callerContracts = byFunction.get(callerFnKey);
      if (!callerContracts) continue;

      for (const calleeName of callees) {
        const calleeContracts = [...byFunction.entries()]
          .filter(([k]) => k.endsWith(`/${calleeName}`))
          .flatMap(([, cs]) => cs);
        if (calleeContracts.length === 0) continue;

        for (const callee of calleeContracts) {
          for (const prop of callee.proven) {
            const calleePre = extractPreconditions(prop.smt2);
            if (!calleePre) continue;

            for (const caller of callerContracts) {
              for (const callerProp of caller.proven) {
                const callerPre = extractPreconditions(callerProp.smt2);
                if (!callerPre) continue;

                const callerVars = extractDeclaredVars(callerProp.smt2);
                const calleeVars = extractDeclaredVars(prop.smt2);

                const callerBlock = namespaceBlock(
                  [...callerPre.decls, ...callerPre.preconditions].join("\n"),
                  callerVars, "caller_"
                );
                const calleeBlock = namespaceBlock(
                  [...calleePre.decls, ...calleePre.preconditions].join("\n"),
                  calleeVars, "callee_"
                );

                const smt2 = `; ENTAILMENT: Does ${caller.function}:${caller.line} establish ${callee.function}:${callee.line}'s preconditions?
; Caller preconditions:
${callerBlock}
; Callee preconditions (negated — if unsat, caller guarantees them):
${calleeBlock.split("\n").map((l) => {
  if (l.trim().startsWith("(assert")) {
    const inner = l.trim().slice("(assert ".length, -1);
    return `(assert (not ${inner}))`;
  }
  return l;
}).join("\n")}

(check-sat)`;

                results.push(runCheck(
                  this.name,
                  `${caller.function}:${caller.line} → ${callee.function}:${callee.line}: ${prop.claim.slice(0, 60)}`,
                  `${caller.function}:${caller.line}`,
                  smt2,
                  "unsat",
                  `${callee.function}:${callee.line}`
                ));
              }
            }
          }
        }
      }
    }

    return results;
  }
}
