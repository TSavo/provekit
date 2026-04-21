import { spawn } from "child_process";
import type { TestAdapter, TestInvocation, TestOutcome, TestOutcomeKind } from "./Adapter";

/**
 * Vitest adapter. Invokes `vitest run <file> [-t regex] --reporter=json`
 * and parses the JSON reporter's `testResults[].assertionResults[]` to
 * determine per-test outcomes. Note: vitest's -t (--testNamePattern) is
 * interpreted as a regex, so we escape the caller's testName before
 * passing it to get predictable substring-style matching.
 */
export class VitestAdapter implements TestAdapter {
  readonly framework = "vitest";
  readonly name = "vitest JSON reporter";

  async runTest(inv: TestInvocation): Promise<TestOutcome> {
    const start = Date.now();
    const args = ["vitest", "run", inv.testFile, "--reporter=json"];
    if (inv.testName) {
      const escaped = inv.testName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      args.push("-t", escaped);
    }

    return runCommand("npx", args, inv.projectRoot, inv.timeoutMs, start, (stdout) => {
      const jsonBlock = extractLastJsonBlock(stdout);
      if (!jsonBlock) {
        return { kind: "adapter-error" as const, message: "no JSON block in vitest output" };
      }
      let report: any;
      try {
        report = JSON.parse(jsonBlock);
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

/**
 * Extracts the last balanced-braces JSON object from a stdout stream.
 * Test frameworks can log objects to stdout before or after the reporter's
 * JSON output, so matching the first brace greedily is fragile. This
 * scans from the end backwards for the matching open-brace that contains
 * a complete object, tolerating pre- and post-reporter log noise.
 */
export function extractLastJsonBlock(stdout: string): string | null {
  const lastBrace = stdout.lastIndexOf("}");
  if (lastBrace < 0) return null;
  let depth = 0;
  for (let i = lastBrace; i >= 0; i--) {
    const ch = stdout[i];
    if (ch === "}") depth++;
    else if (ch === "{") {
      depth--;
      if (depth === 0) return stdout.slice(i, lastBrace + 1);
    }
  }
  return null;
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

    // detached: true on POSIX puts the child in its own process group, so
    // we can SIGKILL the whole group (including npx's grandchild vitest
    // process) via a negative PID. On Windows we can't do this; fall back
    // to the single-process kill.
    const isPosix = process.platform !== "win32";
    let child;
    try {
      child = spawn(cmd, args, { cwd, env: process.env, detached: isPosix });
    } catch (err: any) {
      resolve({
        kind: "adapter-error",
        message: `spawn synchronously failed: ${err?.message?.slice(0, 120) || "unknown"}`,
        durationMs: Date.now() - start,
      });
      return;
    }

    const killTree = () => {
      try {
        if (isPosix && child.pid) process.kill(-child.pid, "SIGKILL");
        else child.kill("SIGKILL");
      } catch {}
    };

    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      killTree();
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

      let parsed: { kind: TestOutcomeKind; message: string };
      try {
        parsed = parse(stdout, stderr, exitCode);
      } catch (e: any) {
        parsed = {
          kind: "adapter-error",
          message: `parse threw: ${e?.message?.slice(0, 160) || "unknown"}`,
        };
      }

      resolve({
        ...parsed,
        durationMs: Date.now() - start,
        rawOutput: stdout.slice(0, 8192),
      });
    });
  });
}
