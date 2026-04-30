/**
 * Scenario dbz-002: division-by-zero — variant with named function expression.
 */
import type { CorpusScenario } from "../../scenarios.js";
import { intZeroFixtureStub } from "../../commonStubs.js";

export const scenario: CorpusScenario = {
  id: "dbz-002",
  bugClass: "division-by-zero",
  files: {
    "src/calc.ts":
      'export const ratio = (x: number, y: number): number => x / y;\n',
  },
  bugReport:
    "Arrow function ratio() divides x by y without checking y. " +
    "src/calc.ts line 1.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "ratio(x, y) may divide by zero",
        failureDescription: "No guard on y in arrow function.",
        fixHint: "Add y === 0 check",
        codeReferences: [{ file: "src/calc.ts", line: 1 }],
        bugClassHint: "divide-by-zero",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [
          { kind: "code_patch", rationale: "Guard divisor" },
          { kind: "regression_test", rationale: "Verify guard" },
        ],
        rationale: "Division by zero is a code invariant violation.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "y may be zero at the division site",
        smt_declarations: ["(declare-const y Int)"],
        smt_violation_assertion: "(assert (= y 0))",
        bindings: [{ smt_constant: "y", source_expr: "y", sort: "Int" }],
        citations: [
          {
            smt_clause: "(= y 0)",
            source_quote: "ratio() divides x by y without checking y",
          },
        ],
      }),
    },
    intZeroFixtureStub("y"),
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Guard prevents division by zero",
            confidence: 0.9,
            patch: {
              description: "Add zero guard to ratio()",
              fileEdits: [
                {
                  file: "src/calc.ts",
                  newContent:
                    'export const ratio = (x: number, y: number): number => {\n' +
                    '  if (y === 0) throw new Error("Division by zero");\n' +
                    '  return x / y;\n' +
                    '};\n',
                },
              ],
            },
          },
        ],
      }),
    },
    {
      matchPrompt: "A bug was just fixed at one site",
      response: JSON.stringify([]),
    },
    {
      matchPrompt: "TypeScript testing expert",
      response: JSON.stringify({
        testCode:
          "import { describe, it, expect } from 'vitest';\n" +
          "describe('ratio', () => {\n" +
          "  it('throws on zero y', () => {\n" +
          "    expect(() => (0 as any)).not.toThrow();\n" +
          "  });\n" +
          "});\n",
        testFilePath: "src/calc.regression.test.ts",
        testName: "regression: ratio throws on zero y",
        witnessInputs: { y: 0 },
      }),
    },
    {
      matchPrompt: "static-analysis rule author",
      response: JSON.stringify({
        kind: "principle_match",
        principleId: "division-by-zero",
      }),
    },
  ],
};
