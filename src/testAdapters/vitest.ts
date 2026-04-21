import { spawn } from "child_process";
import type { TestAdapter, TestInvocation, TestOutcome, TestOutcomeKind } from "./Adapter";

/**
 * Vitest adapter. Invokes `vitest run <file> [-t name] --reporter=json`
 * and parses the JSON reporter's `testResults[].assertionResults[]` to
 * determine per-test outcomes. Filters by testName via vitest's
 * substring match semantics.
 */
export class VitestAdapter implements TestAdapter {
  readonly framework = "vitest";
  readonly name = "vitest JSON reporter";

  async runTest(inv: TestInvocation): Promise<TestOutcome> {
    const start = Date.now();
    const args = ["vitest", "run", inv.testFile, "--reporter=json"];
    if (inv.testName) {
      args.push("-t", inv.testName);
    }

    return runCommand("npx", args, inv.projectRoot, inv.timeoutMs, start, (stdout) => {
      const jsonMatch = stdout.match(/\{[\s\S]*\}/);
      if (!jsonMatch) {
        return { kind: "adapter-error" as const, message: "no JSON block in vitest output" };
      }
      let report: any;
      try {
        report = JSON.parse(jsonMatch[0]);
      } catch (e: any) {
        return { kind: "adapter-error" as const, message: `could not parse vitest JSON: ${e?.message?.slice(0, 80)}` };
      }

      const assertions: Array<{ status: string; title: string; failureMessages?: string[] }> = [];
      for (const tr of report.testResults || []) {
        for (const ar of tr.assertionResults || []) {
          if (!inv.testName || (ar.title || "").includes(inv.testName) || (ar.fullName || "").includes(inv.testName)) {
            assertions.push(ar);
          }
        }
      }

      if (assertions.length === 0) {
        return { kind: "adapter-error" as const, message: inv.testName ? `no test matched "${inv.testName}"` : "no assertions reported" };
      }

      const failures = assertions.filter((a) => a.status === "failed");
      const errors = assertions.filter((a) => a.status === "error" || a.status === "unknown");
      const skipped = assertions.filter((a) => a.status === "skipped" || a.status === "pending" || a.status === "todo");

      if (failures.length > 0) {
        const msg = failures[0]!.failureMessages?.[0] || "assertion failed";
        return { kind: "fail" as const, message: msg.split("\n")[0]!.slice(0, 200) };
      }
      if (errors.length > 0) {
        return { kind: "error" as const, message: "test runtime error" };
      }
      if (skipped.length === assertions.length) {
        return { kind: "skipped" as const, message: "all matching tests skipped" };
      }
      return { kind: "pass" as const, message: `${assertions.length} assertion(s) passed` };
    });
  }
}

export async function runCommand(
  cmd: string,
  args: string[],
  cwd: string,
  timeoutMs: number,
  start: number,
  parse: (stdout: string, stderr: string, exitCode: number | null) => { kind: TestOutcomeKind; message: string }
): Promise<TestOutcome> {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    let settled = false;

    const child = spawn(cmd, args, { cwd, env: process.env });
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      try { child.kill("SIGKILL"); } catch {}
      resolve({
        kind: "timeout",
        message: `adapter killed process after ${timeoutMs}ms`,
        durationMs: Date.now() - start,
        rawOutput: stdout.slice(0, 8192),
      });
    }, timeoutMs);

    child.stdout?.on("data", (buf) => { stdout += buf.toString(); });
    child.stderr?.on("data", (buf) => { stderr += buf.toString(); });

    child.on("error", (err) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({
        kind: "adapter-error",
        message: `spawn failed: ${err.message?.slice(0, 120) || "unknown"}`,
        durationMs: Date.now() - start,
      });
    });

    child.on("close", (exitCode) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      const parsed = parse(stdout, stderr, exitCode);
      resolve({
        ...parsed,
        durationMs: Date.now() - start,
        rawOutput: stdout.slice(0, 8192),
      });
    });
  });
}
