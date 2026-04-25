/**
 * "none" test runner — fallback when no real runner is detected.
 *
 * detect always returns 0.001 (lowest non-zero) so it wins only when nothing
 * else matches and every other runner scored exactly 0.
 *
 * resolveRunnerBinary and invocation both throw — callers that check for
 * runner name "none" (or catch the throw) can surface an informational skip.
 *
 * parseOutcome returns a no-op pass so the audit trail reads cleanly.
 *
 * Self-registers at module load.
 */

import { registerTestRunner } from "./registry.js";

export function registerNone(): void {
  registerTestRunner({
    name: "none",
    description: "No test runner detected; oracle #9 mutation verification will be skipped (informational)",
    detect: (_projectRoot) => 0.001,
    resolveRunnerBinary: (_projectRoot) => {
      throw new Error(
        "no test runner detected; oracle #9 mutation verification skipped (informational)",
      );
    },
    invocation: (_testFilePath) => {
      throw new Error(
        "no test runner detected; oracle #9 mutation verification skipped (informational)",
      );
    },
    parseOutcome: (_exitCode, _stdout, _stderr) => ({
      passed: true,
      testCount: 0,
      details: "no test runner detected; oracle #9 mutation verification skipped (informational)",
    }),
  });
}

registerNone();
