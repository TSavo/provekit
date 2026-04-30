/**
 * Scenario adv-out-of-scope: bug report describes a problem outside provekit's
 * scope (e.g., a network flakiness issue — infrastructure, not code).
 * Classify should route to out_of_scope.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: "adv-out-of-scope",
  bugClass: "novel",
  files: {
    "src/server.ts":
      'export function startServer(port: number): void {\n' +
      '  console.log(`Listening on ${port}`);\n' +
      '}\n',
  },
  bugReport:
    "The AWS Lambda function times out intermittently during peak traffic " +
    "because the upstream RDS connection pool is exhausted. This is a " +
    "database infrastructure scaling problem, not a code bug.",
  expected: {
    completes: [],
    fails: {
      stage: "classify",
      reason: "bug describes infrastructure/scaling problem outside code invariant scope",
    },
    outcome: "out_of_scope",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: "Lambda timeout due to RDS connection pool exhaustion",
        failureDescription: "Infrastructure scaling issue — Lambda can't get a DB connection.",
        codeReferences: [],
        bugClassHint: "infrastructure",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "out_of_scope",
        secondaryLayers: [],
        artifacts: [],
        rationale: "Infrastructure scaling problem. provekit does not fix cloud configuration.",
      }),
    },
  ],
};
