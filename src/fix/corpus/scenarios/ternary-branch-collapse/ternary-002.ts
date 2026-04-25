/**
 * Scenario ternary-002: ternary-branch-collapse — condition is always true.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "ternary-002",
  bugClass: "ternary-branch-collapse",
  files: {
    "src/status.ts":
      'export function getStatus(code: number): string {\n' +
      '  return code >= 0 ? "ok" : "ok";\n' +
      '}\n',
  },
  bugReport:
    "Ternary in status.ts: getStatus() returns 'ok' regardless of code. " +
    "The false branch is unreachable. src/status.ts line 2.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "getStatus() ternary always returns 'ok'",
        failureDescription: "False branch is unreachable — status code has no effect.",
        fixHint: "Differentiate return values between branches",
        codeReferences: [{ file: "src/status.ts", line: 2 }],
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
          { kind: "regression_test", rationale: "Verify distinct branches" },
        ],
        rationale: "Degenerate ternary is a code invariant violation.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "both branches of ternary return same literal",
        smt_declarations: [
          "(declare-const trueResult String)",
          "(declare-const falseResult String)",
        ],
        smt_violation_assertion: "(assert (= trueResult falseResult))",
        bindings: [
          { smt_constant: "trueResult", source_expr: "? branch", sort: "String" },
          { smt_constant: "falseResult", source_expr: ": branch", sort: "String" },
        ],
      }),
    },
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Return distinct status strings",
            confidence: 0.9,
            patch: {
              description: "Fix degenerate ternary in getStatus()",
              fileEdits: [
                {
                  file: "src/status.ts",
                  newContent:
                    'export function getStatus(code: number): string {\n' +
                    '  return code >= 0 ? "ok" : "error";\n' +
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
          "describe('getStatus', () => {\n" +
          "  it('returns error for negative code', () => {\n" +
          "    expect((0 as any)).toBe(undefined);\n" +
          "  });\n" +
          "});\n",
        testFilePath: "src/status.regression.test.ts",
        testName: "regression: getStatus ternary branches distinct",
        witnessInputs: { code: -1 },
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
