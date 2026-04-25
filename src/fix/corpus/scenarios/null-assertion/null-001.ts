/**
 * Scenario null-001: null-assertion — dereference without null check.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "null-001",
  bugClass: "null-assertion",
  files: {
    "src/user.ts":
      'export function getDisplayName(user: { name: string } | null): string {\n' +
      '  return user.name;\n' +
      '}\n',
  },
  bugReport:
    "Null dereference in user.ts: getDisplayName() accesses user.name without " +
    "checking user !== null. src/user.ts line 2.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "getDisplayName() dereferences user without null check",
        failureDescription: "user.name accessed without checking user !== null.",
        fixHint: "Add null guard before property access",
        codeReferences: [{ file: "src/user.ts", line: 2 }],
        bugClassHint: "null-assertion",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [
          { kind: "code_patch", rationale: "Add null guard" },
          { kind: "regression_test", rationale: "Verify null safety" },
        ],
        rationale: "Null dereference is a code invariant violation.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "user may be null at dereference site",
        smt_declarations: ["(declare-const userIsNull Bool)"],
        smt_violation_assertion: "(assert (= userIsNull true))",
        bindings: [{ smt_constant: "userIsNull", source_expr: "user === null", sort: "Bool" }],
      }),
    },
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Null guard prevents dereference",
            confidence: 0.95,
            patch: {
              description: "Add null guard to getDisplayName()",
              fileEdits: [
                {
                  file: "src/user.ts",
                  newContent:
                    'export function getDisplayName(user: { name: string } | null): string {\n' +
                    '  if (user === null) throw new Error("user is null");\n' +
                    '  return user.name;\n' +
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
          "describe('getDisplayName', () => {\n" +
          "  it('throws on null user', () => {\n" +
          "    expect(() => (0 as any)).not.toThrow();\n" +
          "  });\n" +
          "});\n",
        testFilePath: "src/user.regression.test.ts",
        testName: "regression: getDisplayName throws on null user",
        witnessInputs: { userIsNull: true },
      }),
    },
    {
      matchPrompt: "static-analysis rule author",
      response: JSON.stringify({
        kind: "principle_match",
        principleId: "null-assertion",
      }),
    },
  ],
};
