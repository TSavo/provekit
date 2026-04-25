/**
 * Scenario adv-missing-file: bug report references a file that doesn't exist.
 * Locate stage should fail (locus = null → exit code 2).
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "adv-missing-file",
  bugClass: "novel",
  files: {
    "src/real.ts":
      'export function add(a: number, b: number): number { return a + b; }\n',
  },
  bugReport:
    "Division by zero in src/nonexistent.ts line 5 function badDivide().",
  expected: {
    completes: [],
    fails: {
      stage: "locate",
      reason: "file referenced in bug report does not exist in SAST DB",
    },
    outcome: "out_of_scope",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "Division by zero in nonexistent.ts",
        failureDescription: "badDivide() divides by zero.",
        fixHint: "Add guard",
        codeReferences: [{ file: "src/nonexistent.ts", line: 5, function: "badDivide" }],
        bugClassHint: "divide-by-zero",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "Division by zero.",
      }),
    },
  ],
};
