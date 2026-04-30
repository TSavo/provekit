/**
 * Scenario adv-unsatisfiable-invariant: LLM hallucinates an unsatisfiable invariant.
 * Oracle #1 (Z3 SAT check) should reject: the violation state is UNSAT before any fix.
 * Expected: C1 fails — the invariant is not reachable.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "adv-unsatisfiable-invariant",
  bugClass: "novel",
  files: {
    "src/guard.ts":
      'export function safeDiv(a: number, b: number): number {\n' +
      '  if (b === 0) throw new Error("zero divisor");\n' +
      '  return a / b;\n' +
      '}\n',
  },
  bugReport:
    "Division by zero in src/guard.ts line 3. safeDiv() may divide by zero.",
  expected: {
    completes: [],
    fails: {
      stage: "C1",
      reason: "LLM-proposed invariant is UNSAT (violation not reachable — guard already present)",
    },
    outcome: "rejected",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "safeDiv() may divide by zero",
        failureDescription: "b could be zero at division site.",
        fixHint: "Add guard",
        codeReferences: [{ file: "src/guard.ts", line: 3 }],
        bugClassHint: "divide-by-zero",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [{ kind: "code_patch", rationale: "Guard divisor" }],
        rationale: "Division by zero.",
      }),
    },
    {
      // C1 LLM returns an invariant where violation is UNSAT — contradiction in itself.
      // "(and (= b 0) (not (= b 0)))" is always false, so Z3 returns UNSAT.
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "b must be both zero and non-zero simultaneously",
        smt_declarations: ["(declare-const b Int)"],
        // This assertion is a contradiction — UNSAT means no violation is reachable.
        smt_violation_assertion: "(assert (and (= b 0) (not (= b 0))))",
        bindings: [{ smt_constant: "b", source_expr: "b", sort: "Int" }],
      }),
    },
  ],
};
