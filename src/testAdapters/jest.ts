import type { TestAdapter, TestInvocation, TestOutcome } from "./Adapter";
import { runCommand, extractLastJsonBlock } from "./vitest";

/**
 * Jest adapter. Invokes `npx jest <file> [-t name] --json --silent` and
 * parses the JSON output (testResults/assertionResults). Jest's -t flag
 * is a regex, so we escape the testName to make substring matching
 * predictable; callers who want regex semantics can pass pre-escaped
 * patterns.
 */
export class JestAdapter implements TestAdapter {
  readonly framework = "jest";
  readonly name = "jest JSON output";

  async runTest(inv: TestInvocation): Promise<TestOutcome> {
    const start = Date.now();
    const args = ["jest", inv.testFile, "--json", "--silent"];
    if (inv.testName) {
      const escaped = inv.testName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      args.push("-t", escaped);
    }

    return runCommand("npx", args, inv.projectRoot, inv.timeoutMs, start, (stdout) => {
      const jsonBlock = extractLastJsonBlock(stdout);
      if (!jsonBlock) {
        return { kind: "adapter-error" as const, message: "no JSON block in jest output" };
      }
      let report: any;
      try {
        report = JSON.parse(jsonBlock);
      } catch (e: any) {
        return { kind: "adapter-error" as const, message: `could not parse jest JSON: ${e?.message?.slice(0, 80)}` };
      }

      const assertions: Array<{ status: string; title: string; failureMessages?: string[] }> = [];
      for (const tr of report.testResults || []) {
        for (const ar of tr.assertionResults || []) {
          assertions.push(ar);
        }
      }

      if (assertions.length === 0) {
        return { kind: "adapter-error" as const, message: inv.testName ? `no test matched "${inv.testName}"` : "no assertions reported" };
      }

      const failures = assertions.filter((a) => a.status === "failed");
      const skipped = assertions.filter((a) => a.status === "skipped" || a.status === "pending" || a.status === "todo");
      const passed = assertions.filter((a) => a.status === "passed");

      if (failures.length > 0) {
        const msg = failures[0]!.failureMessages?.[0] || "assertion failed";
        return { kind: "fail" as const, message: msg.split("\n")[0]!.slice(0, 200) };
      }
      if (passed.length === 0 && skipped.length > 0) {
        return { kind: "skipped" as const, message: "all matching tests skipped" };
      }
      if (passed.length === 0) {
        return { kind: "adapter-error" as const, message: "no tests passed or failed — runtime anomaly" };
      }
      return { kind: "pass" as const, message: `${passed.length} assertion(s) passed` };
    });
  }
}
