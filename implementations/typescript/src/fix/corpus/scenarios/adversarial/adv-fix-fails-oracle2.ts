/**
 * Scenario adv-fix-fails-oracle2: LLM proposes a fix that doesn't satisfy oracle #2.
 * The invariant holds pre-fix (SAT), but the proposed patch doesn't make it UNSAT.
 * Expected: C3 fails — oracle #2 rejects the fix (invariant still holds after patch).
 *
 * We simulate this by having the patch not actually fix the bug —
 * the division still occurs with no guard.
 */
import type { CorpusScenario } from "../../scenarios.js";
import { intZeroFixtureStub } from "../../commonStubs.js";

export const scenario: CorpusScenario = {
  id: "adv-fix-fails-oracle2",
  bugClass: "division-by-zero",
  files: {
    "src/bad-fix.ts":
      'export function divide(a: number, b: number): number {\n' +
      '  return a / b;\n' +
      '}\n',
  },
  bugReport:
    "Division by zero in src/bad-fix.ts line 2. divide() doesn't guard b.",
  expected: {
    completes: ["C1", "C2"],
    fails: {
      stage: "C3",
      reason: "proposed fix does not satisfy oracle #2 — invariant still reachable post-patch",
    },
    outcome: "rejected",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "divide() returns Infinity when b is zero",
        failureDescription: "No guard on divisor.",
        fixHint: "Add if (b === 0) check",
        codeReferences: [{ file: "src/bad-fix.ts", line: 2 }],
        bugClassHint: "divide-by-zero",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [{ kind: "code_patch", rationale: "Guard divisor" }],
        rationale: "Division by zero is a code invariant violation.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify({
        description: "b may be zero at division site",
        smt_declarations: ["(declare-const b Int)"],
        smt_violation_assertion: "(assert (= b 0))",
        bindings: [{ smt_constant: "b", source_expr: "b", sort: "Int" }],
        citations: [
          {
            smt_clause: "(= b 0)",
            source_quote: "divide() doesn't guard b",
          },
        ],
      }),
    },
    intZeroFixtureStub("b"),
    {
      // Patch does NOT add a guard — bug still present after applying.
      matchPrompt: "propose up to",
      response: JSON.stringify({
        candidates: [
          {
            rationale: "Added a comment explaining the behavior (not a real fix)",
            confidence: 0.3,
            patch: {
              description: "Added comment (no real fix)",
              fileEdits: [
                {
                  file: "src/bad-fix.ts",
                  newContent:
                    '// NOTE: b may be zero\n' +
                    'export function divide(a: number, b: number): number {\n' +
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
  ],
};
