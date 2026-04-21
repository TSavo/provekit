import type { TestAdapter, TestInvocation, TestOutcome } from "./Adapter";
import { runCommand, extractLastJsonBlock } from "./vitest";

/**
 * Mocha adapter. Invokes `npx mocha <file> [--grep name] --reporter json`.
 * Mocha's JSON reporter emits a single report object to stdout; we parse
 * stats.passes/failures/pending plus the failures[] array for failure
 * detail.
 *
 * Note: mocha's --grep uses regex. Like the jest adapter, we escape the
 * testName for predictable substring matching.
 */
export class MochaAdapter implements TestAdapter {
  readonly framework = "mocha";
  readonly name = "mocha JSON reporter";

  async runTest(inv: TestInvocation): Promise<TestOutcome> {
    const start = Date.now();
    const args = ["mocha", inv.testFile, "--reporter", "json"];
    if (inv.testName) {
      const escaped = inv.testName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      args.push("--grep", escaped);
    }

    return runCommand("npx", args, inv.projectRoot, inv.timeoutMs, start, (stdout) => {
      const jsonBlock = extractLastJsonBlock(stdout);
      if (!jsonBlock) {
        return { kind: "adapter-error" as const, message: "no JSON block in mocha output" };
      }
      let report: any;
      try {
        report = JSON.parse(jsonBlock);
      } catch (e: any) {
        return { kind: "adapter-error" as const, message: `could not parse mocha JSON: ${e?.message?.slice(0, 80)}` };
      }

      const stats = report.stats || {};
      const passes = stats.passes || 0;
      const failures = stats.failures || 0;
      const pending = stats.pending || 0;
      const tests = stats.tests || 0;

      if (tests === 0) {
        return { kind: "adapter-error" as const, message: inv.testName ? `no test matched "${inv.testName}"` : "no tests ran" };
      }

      if (failures > 0) {
        const firstFail = (report.failures || [])[0];
        const msg = firstFail?.err?.message || firstFail?.err?.stack?.split("\n")[0] || "assertion failed";
        return { kind: "fail" as const, message: msg.split("\n")[0].slice(0, 200) };
      }
      if (passes === 0 && pending === tests) {
        return { kind: "skipped" as const, message: "all matching tests pending/skipped" };
      }
      if (passes === 0) {
        return { kind: "adapter-error" as const, message: "no tests passed or failed — runtime anomaly" };
      }
      return { kind: "pass" as const, message: `${passes} test(s) passed` };
    });
  }
}
