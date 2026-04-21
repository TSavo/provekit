import type { TestAdapter, TestInvocation, TestOutcome } from "./Adapter";
import { runCommand } from "./vitest";

/**
 * Node built-in test runner adapter. Invokes `node --test <file>` and
 * parses the TAP output. Node's test runner doesn't have a name-filter
 * CLI flag equivalent to -t/--grep, so when testName is provided we
 * run the whole file and filter the results by test name afterwards.
 *
 * Detects the TAP summary (# tests, # pass, # fail) for an overall
 * verdict and scans individual test lines for name-specific outcomes.
 */
export class NodeTestAdapter implements TestAdapter {
  readonly framework = "node-test";
  readonly name = "node --test TAP output";

  async runTest(inv: TestInvocation): Promise<TestOutcome> {
    const start = Date.now();
    const args = ["--test", inv.testFile];

    return runCommand("node", args, inv.projectRoot, inv.timeoutMs, start, (stdout, _stderr, exitCode) => {
      const lines = stdout.split("\n");

      // TAP lines: "ok 42 - <name>" or "not ok 42 - <name>"
      const testLineRe = /^(ok|not ok)\s+\d+\s*(?:-\s*)?(.*?)(?:\s+#\s*(.+))?$/;

      let passed = 0, failed = 0, skipped = 0;
      const nameFilter = inv.testName;
      const matchingLines: string[] = [];

      for (const line of lines) {
        const m = line.match(testLineRe);
        if (!m) continue;
        const [, status, name, directive] = m;
        if (nameFilter && !name!.includes(nameFilter)) continue;

        matchingLines.push(line);
        if (directive && /SKIP|TODO/i.test(directive)) skipped++;
        else if (status === "ok") passed++;
        else failed++;
      }

      const total = passed + failed + skipped;
      if (total === 0) {
        return { kind: "adapter-error" as const, message: nameFilter ? `no test matched "${nameFilter}"` : "no TAP test lines parsed" };
      }

      if (failed > 0) {
        const failureLine = matchingLines.find((l) => l.startsWith("not ok")) || "";
        return { kind: "fail" as const, message: failureLine.slice(0, 200) || "assertion failed" };
      }
      if (passed === 0 && skipped > 0) {
        return { kind: "skipped" as const, message: "all matching tests skipped" };
      }
      if (passed === 0) {
        return { kind: "adapter-error" as const, message: `node --test returned exit ${exitCode} with no passing tests` };
      }
      return { kind: "pass" as const, message: `${passed} test(s) passed` };
    });
  }
}
