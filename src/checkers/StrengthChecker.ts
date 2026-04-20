import { Contract } from "../contracts";
import { Checker, CheckResult } from "./Checker";
import { verifyBlock } from "../verifier";

export class StrengthChecker implements Checker {
  readonly name = "strength";

  check(contracts: Contract[]): CheckResult[] {
    const results: CheckResult[] = [];

    for (const contract of contracts) {
      for (const proven of contract.proven) {
        const r = this.evaluateStrength(proven.smt2);
        if (r.totalAsserts === 0) continue;

        const description =
          `[${proven.principle || "?"}] ${contract.function}:${contract.line}: ` +
          `${r.loadBearing}/${r.totalAsserts} assertions load-bearing — ${proven.claim.slice(0, 60)}`;

        const verdict: "proven" | "violation" | "error" =
          r.status === "error" ? "error" :
          r.loadBearing > 0 ? "proven" : "violation";

        results.push({
          checker: this.name,
          description,
          sourceContract: `${contract.function}:${contract.line}`,
          smt2: proven.smt2,
          expected: "unsat",
          z3Result: r.status === "error" ? "error" : "unsat",
          verdict,
          error: r.status === "error" ? "strength evaluation errored" : undefined,
        });
      }
    }

    return results;
  }

  private evaluateStrength(smt2: string): {
    loadBearing: number;
    totalAsserts: number;
    status: "ok" | "error";
  } {
    const lines = smt2.split("\n");
    const assertIndices: number[] = [];
    for (let i = 0; i < lines.length; i++) {
      const trimmed = lines[i]!.trim();
      if (!trimmed.startsWith("(assert")) continue;
      let depth = 0;
      for (const ch of lines[i]!) {
        if (ch === "(") depth++;
        else if (ch === ")") depth--;
      }
      if (depth === 0) assertIndices.push(i);
    }

    if (assertIndices.length === 0) {
      return { loadBearing: 0, totalAsserts: 0, status: "ok" };
    }

    let loadBearing = 0;
    let errored = false;

    for (const idx of assertIndices) {
      const without = lines.filter((_, i) => i !== idx).join("\n");
      const { result } = verifyBlock(without);
      if (result === "unsat") {
        // still unsat without this assert → not load-bearing
      } else if (result === "sat") {
        loadBearing++;
      } else {
        errored = true;
      }
    }

    return {
      loadBearing,
      totalAsserts: assertIndices.length,
      status: errored && loadBearing === 0 ? "error" : "ok",
    };
  }
}
