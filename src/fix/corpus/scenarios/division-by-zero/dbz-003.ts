/**
 * Scenario dbz-003: division-by-zero — class method variant.
 */
import type { CorpusScenario } from "../../scenarios.js";
import { intZeroFixtureStub } from "../../commonStubs.js";

export const scenario: CorpusScenario = {
  id: "dbz-003",
  bugClass: "division-by-zero",
  files: {
    "src/stats.ts":
      'export class Stats {\n' +
      '  mean(sum: number, count: number): number {\n' +
      '    return sum / count;\n' +
      '  }\n' +
      '}\n',
  },
  bugReport:
    "Stats.mean() divides sum by count without checking count === 0. " +
    "When called with an empty dataset, returns NaN. src/stats.ts line 3.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "Stats.mean() returns NaN when count is zero",
        failureDescription: "No guard on count in Stats.mean().",
        fixHint: "Add count === 0 check",
        codeReferences: [{ file: "src/stats.ts", line: 3 }],
        bugClassHint: "divide-by-zero",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [
          { kind: "code_patch", rationale: "Guard count" },
          { kind: "regression_test", rationale: "Verify guard" },
        ],
        rationale: "Division by zero is a code invariant violation.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "count may be zero at the division site",
        smt_declarations: ["(declare-const count Int)"],
        smt_violation_assertion: "(assert (= count 0))",
        bindings: [{ smt_constant: "count", source_expr: "count", sort: "Int" }],
        citations: [
          {
            smt_clause: "(= count 0)",
            source_quote: "Stats.mean() divides sum by count without checking count === 0",
          },
        ],
      }),
    },
    intZeroFixtureStub("count"),
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Guard prevents division by zero in Stats.mean",
            confidence: 0.95,
            patch: {
              description: "Add zero guard to Stats.mean()",
              fileEdits: [
                {
                  file: "src/stats.ts",
                  newContent:
                    'export class Stats {\n' +
                    '  mean(sum: number, count: number): number {\n' +
                    '    if (count === 0) throw new Error("Cannot compute mean of empty dataset");\n' +
                    '    return sum / count;\n' +
                    '  }\n' +
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
          "describe('Stats.mean', () => {\n" +
          "  it('throws on empty dataset', () => {\n" +
          "    expect(() => (0 as any)).not.toThrow();\n" +
          "  });\n" +
          "});\n",
        testFilePath: "src/stats.regression.test.ts",
        testName: "regression: Stats.mean throws on zero count",
        witnessInputs: { count: 0 },
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
