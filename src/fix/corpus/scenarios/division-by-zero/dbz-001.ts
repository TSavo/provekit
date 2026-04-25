/**
 * Scenario dbz-001: division-by-zero — basic guard pattern.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "dbz-001",
  bugClass: "division-by-zero",
  files: {
    "src/math.ts":
      'export function divide(a: number, b: number): number {\n' +
      '  return a / b;\n' +
      '}\n',
  },
  bugReport:
    "Division by zero in math.ts: calling divide(x, 0) returns Infinity. " +
    "Found at src/math.ts line 2.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "divide(a, b) returns Infinity when b is zero",
        failureDescription: "No guard on divisor causes divide-by-zero.",
        fixHint: "Add if (b === 0) throw check",
        codeReferences: [{ file: "src/math.ts", line: 2 }],
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
        description: "b may be zero at the division site",
        smt_declarations: [
          "(declare-const b Int)",
        ],
        smt_violation_assertion: "(assert (= b 0))",
        bindings: [
          { smt_constant: "b", source_expr: "b", sort: "Int" },
        ],
      }),
    },
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Guard prevents division by zero",
            confidence: 0.9,
            patch: {
              description: "Add zero guard to divide()",
              fileEdits: [
                {
                  file: "src/math.ts",
                  newContent:
                    'export function divide(a: number, b: number): number {\n' +
                    '  if (b === 0) throw new Error("Division by zero");\n' +
                    '  return a / b;\n' +
                    '}\n',
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
          "describe('divide', () => {\n" +
          "  it('throws on zero divisor', () => {\n" +
          "    expect(() => (0 as any)).not.toThrow();\n" +
          "  });\n" +
          "});\n",
        testFilePath: "src/math.regression.test.ts",
        testName: "regression: divide throws on zero divisor",
        witnessInputs: { b: 0 },
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
