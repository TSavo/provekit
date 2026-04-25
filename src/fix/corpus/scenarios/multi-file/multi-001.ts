/**
 * Scenario multi-001: multi-file bug.
 * Division by zero spans two files: compute.ts calls divide.ts.
 * C4 complementary changes should fire on both.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "multi-001",
  bugClass: "division-by-zero",
  files: {
    "src/divide.ts":
      'export function divide(a: number, b: number): number {\n' +
      '  return a / b;\n' +
      '}\n',
    "src/compute.ts":
      'import { divide } from "./divide.js";\n' +
      'export function compute(x: number, y: number): number {\n' +
      '  return divide(x, y);\n' +
      '}\n',
  },
  bugReport:
    "Division by zero bug: calling divide(x, 0) produces Infinity/NaN. " +
    "Affects compute() when y is zero. Found at src/divide.ts line 2.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "divide(a, b) returns Infinity/NaN when b is zero",
        failureDescription: "Calling divide(x, 0) produces Infinity or NaN.",
        fixHint: "Add a zero guard before dividing",
        codeReferences: [{ file: "src/divide.ts", line: 2 }],
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
        smt_declarations: ["(declare-const b Int)"],
        smt_violation_assertion: "(assert (= b 0))",
        bindings: [{ smt_constant: "b", source_expr: "b", sort: "Int" }],
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
                  file: "src/divide.ts",
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
      // C4: complementary changes in the caller file (compute.ts) — caller_update kind.
      matchPrompt: "A bug was just fixed at one site",
      response: JSON.stringify([
        {
          kind: "caller_update",
          targetNodeId: "compute-node-stub",
          patch: {
            description: "Add error handling in compute() caller",
            fileEdits: [
              {
                file: "src/compute.ts",
                newContent:
                  'import { divide } from "./divide.js";\n' +
                  'export function compute(x: number, y: number): number {\n' +
                  '  if (y === 0) throw new Error("compute: y must be non-zero");\n' +
                  '  return divide(x, y);\n' +
                  '}\n',
              },
            ],
          },
          rationale: "Caller should guard before calling divide()",
          verifiedAgainstOverlay: true,
          overlayZ3Verdict: "unsat",
          priority: 1,
          audit: {
            siteKind: "caller_update",
            discoveredVia: "calls_table",
            z3RunMs: 5,
          },
        },
      ]),
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
        testFilePath: "src/divide.regression.test.ts",
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
