/**
 * Scenario null-002: null-assertion — optional chain missing on deep access.
 */
import type { CorpusScenario } from "../../scenarios.js";
import { boolFixtureStub } from "../../commonStubs.js";

export const scenario: CorpusScenario = {
  id: "null-002",
  bugClass: "null-assertion",
  files: {
    "src/config.ts":
      'export function getPort(config: { server?: { port: number } } | null): number {\n' +
      '  return config.server.port;\n' +
      '}\n',
  },
  bugReport:
    "Null dereference in config.ts: getPort() accesses config.server.port " +
    "without checking config or config.server. src/config.ts line 2.",
  expected: {
    completes: ["C1", "C2", "C3", "C4", "C5", "C6", "D1"],
    outcome: "applied",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "getPort() dereferences config.server.port without null checks",
        failureDescription: "config or config.server may be null/undefined.",
        fixHint: "Use optional chaining or explicit null checks",
        codeReferences: [{ file: "src/config.ts", line: 2 }],
        bugClassHint: "null-assertion",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [
          { kind: "code_patch", rationale: "Add null guards" },
          { kind: "regression_test", rationale: "Verify null safety" },
        ],
        rationale: "Null dereference is a code invariant violation.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "config.server may be undefined at dereference",
        smt_declarations: ["(declare-const serverDefined Bool)"],
        smt_violation_assertion: "(assert (= serverDefined false))",
        bindings: [{ smt_constant: "serverDefined", source_expr: "config.server !== undefined", sort: "Bool" }],
        citations: [
          {
            smt_clause: "(= serverDefined false)",
            source_quote: "getPort() accesses config.server.port without checking config or config.server",
          },
        ],
      }),
    },
    boolFixtureStub("serverDefined", false),
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Guard prevents null dereference on config.server",
            confidence: 0.9,
            patch: {
              description: "Add null guards to getPort()",
              fileEdits: [
                {
                  file: "src/config.ts",
                  newContent:
                    'export function getPort(config: { server?: { port: number } } | null): number {\n' +
                    '  if (config === null || config.server === undefined) {\n' +
                    '    throw new Error("config or config.server is missing");\n' +
                    '  }\n' +
                    '  return config.server.port;\n' +
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
          "describe('getPort', () => {\n" +
          "  it('throws when server is missing', () => {\n" +
          "    expect(() => (0 as any)).not.toThrow();\n" +
          "  });\n" +
          "});\n",
        testFilePath: "src/config.regression.test.ts",
        testName: "regression: getPort throws when server missing",
        witnessInputs: { serverDefined: false },
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
