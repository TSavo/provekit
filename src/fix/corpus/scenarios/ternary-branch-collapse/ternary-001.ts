/**
 * Scenario ternary-001: ternary-branch-collapse — both branches identical.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "ternary-001",
  bugClass: "ternary-branch-collapse",
  files: {
    "src/flag.ts":
      'export function getMode(debug: boolean): string {\n' +
      '  return debug ? "production" : "production";\n' +
      '}\n',
  },
  bugReport:
    "Ternary branches collapse in flag.ts: both branches of the ternary " +
    "in getMode() return the same value. src/flag.ts line 2.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "getMode() ternary both branches return 'production'",
        failureDescription: "debug flag has no effect — ternary is degenerate.",
        fixHint: "Fix the ternary to return different values per branch",
        codeReferences: [{ file: "src/flag.ts", line: 2 }],
        bugClassHint: "ternary-branch-collapse",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [
          { kind: "code_patch", rationale: "Fix collapsed ternary" },
          { kind: "regression_test", rationale: "Verify branch distinction" },
        ],
        rationale: "Degenerate ternary is a code invariant violation.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "both ternary branches produce same value",
        smt_declarations: [
          "(declare-const trueResult String)",
          "(declare-const falseResult String)",
        ],
        smt_violation_assertion: "(assert (= trueResult falseResult))",
        bindings: [
          { smt_constant: "trueResult", source_expr: "debug ? branch", sort: "String" },
          { smt_constant: "falseResult", source_expr: ": branch", sort: "String" },
        ],
      }),
    },
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Fix collapsed ternary to return correct values",
            confidence: 0.95,
            patch: {
              description: "Fix degenerate ternary in getMode()",
              fileEdits: [
                {
                  file: "src/flag.ts",
                  newContent:
                    'export function getMode(debug: boolean): string {\n' +
                    '  return debug ? "development" : "production";\n' +
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
          "describe('getMode', () => {\n" +
          "  it('returns different values per branch', () => {\n" +
          "    expect((0 as any)).toBe(undefined);\n" +
          "  });\n" +
          "});\n",
        testFilePath: "src/flag.regression.test.ts",
        testName: "regression: getMode ternary branches differ",
        witnessInputs: { debug: true },
      }),
    },
    {
      matchPrompt: "static-analysis rule author",
      response: JSON.stringify({
        kind: "principle_match",
        principleId: "ternary-branch-collapse",
      }),
    },
  ],
};
