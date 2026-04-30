import type { TestAdapter, TestInvocation, TestOutcome } from "./Adapter";
import { runCommand } from "./vitest";

/**
 * Node built-in test runner adapter. Invokes
 *   node --test --test-reporter=tap [--test-name-pattern=...] <file>
 *
 * Two notes:
 *   1. Node 23+ changed the default reporter to `spec`; TAP is no
 *      longer the default, so we request it explicitly.
 *   2. Node's runner supports --test-name-pattern (added in 18.17 /
 *      20.0) with regex semantics. We escape the caller's testName
 *      to get substring behaviour predictable across versions; the
 *      fallback post-hoc TAP-line filter still runs for older Node
 *      versions that ignore the flag.
 */
export class NodeTestAdapter implements TestAdapter {
  readonly framework = "node-test";
  readonly name = "node --test TAP output";

  async runTest(inv: TestInvocation): Promise<TestOutcome> {
    const start = Date.now();
    const args = ["--test", "--test-reporter=tap"];
    if (inv.testName) {
      const escaped = inv.testName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      args.push(`--test-name-pattern=${escaped}`);
    }
    args.push(inv.testFile);

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
