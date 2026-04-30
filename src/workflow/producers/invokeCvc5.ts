/**
 * invoke-cvc5 Stage — prove-cross-solver workflow's CVC5 solver
 * invocation step.
 *
 * Spawns `cvc5 -L smt2 --produce-models`, writes SMT-LIB to stdin,
 * parses sat/unsat. Stage rather than Action because the verdict is a
 * content-addressable claim about a deterministic input: identical
 * SMT-LIB + identical cvc5 version produces the identical verdict, so
 * it caches. This is the same rationale as invokeZ3 — see
 * docs/specs/2026-04-29-stages-vs-actions.md and the docstring on
 * src/workflow/producers/invokeZ3.ts.
 *
 * The two solver Stages take the SAME IR (different SMT-LIB byte
 * streams), produce DIFFERENT producer-id mementos, and both reference
 * the SAME IR CID as their input. The cross-solver workflow is the
 * operational test of the architectural claim that propertyHash CIDs
 * compose across solver verdicts.
 *
 * Verdict mapping (refute semantics — the SMT-LIB asserts the NEGATION
 * for unsat-check, identical to invokeZ3):
 *
 *   cvc5 returns    | property verdict | meaning
 *   ----------------+------------------+--------------------------------
 *   unsat           | holds            | no counterexample exists
 *   sat             | violated         | model is the counterexample
 *   timeout / unkn. | undecidable      | solver could not resolve
 *
 * If cvc5 isn't installed, run() throws an InvokeCvc5Error mentioning
 * `cvc5` so consumers know to install it. The cross-solver workflow
 * test gracefully skips when the binary is unavailable on PATH.
 */

import { spawn } from "child_process";
import type { Stage } from "../types.js";

export const INVOKE_CVC5_CAPABILITY = "invoke-cvc5";

export type Cvc5Verdict = "sat" | "unsat" | "unknown" | "timeout";

export interface InvokeCvc5StageInput {
  smtLib: string;
  /** Solver timeout in milliseconds. Default 30_000. */
  timeoutMs?: number;
  /** Override the cvc5 binary path. Default: "cvc5" (PATH lookup). */
  binary?: string;
}

export interface InvokeCvc5StageOutput {
  cvc5Verdict: Cvc5Verdict;
  /** Raw stdout from cvc5. Empty string on timeout. */
  stdout: string;
  /** Raw stderr from cvc5. */
  stderr: string;
  /** Wall-clock duration of the cvc5 invocation. */
  cvc5RunMs: number;
}

export interface MakeInvokeCvc5StageDeps {
  /** Override producer identity. Default: "cvc5-symbolic@unknown". */
  producerVersion?: string;
  /**
   * Spawn function override. Test-only seam — production passes the real
   * `child_process.spawn`.
   */
  spawnFn?: typeof spawn;
}

export class InvokeCvc5Error extends Error {
  constructor(message: string) {
    super(message);
    this.name = "InvokeCvc5Error";
  }
}

interface RunResult {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
}

function runCvc5(
  spawnFn: typeof spawn,
  binary: string,
  args: string[],
  stdin: string,
  timeoutMs: number,
): Promise<RunResult> {
  return new Promise((resolve, reject) => {
    let child;
    try {
      child = spawnFn(binary, args, { stdio: ["pipe", "pipe", "pipe"] });
    } catch (err) {
      reject(
        new InvokeCvc5Error(
          `failed to spawn cvc5 (${binary}): ${(err as Error).message} — install cvc5 to use the prove-cross-solver workflow`,
        ),
      );
      return;
    }

    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const stdoutStream = child.stdout;
    const stderrStream = child.stderr;
    if (stdoutStream) stdoutStream.on("data", (chunk) => (stdout += chunk.toString()));
    if (stderrStream) stderrStream.on("data", (chunk) => (stderr += chunk.toString()));

    const timer = setTimeout(() => {
      timedOut = true;
      try {
        child.kill("SIGKILL");
      } catch {
        // ignore
      }
    }, timeoutMs);

    child.on("error", (err) => {
      clearTimeout(timer);
      const code = (err as NodeJS.ErrnoException).code;
      if (code === "ENOENT") {
        reject(
          new InvokeCvc5Error(
            `cvc5 binary not found in PATH (${binary}) — install cvc5 to use the prove-cross-solver workflow`,
          ),
        );
        return;
      }
      reject(new InvokeCvc5Error(`cvc5 spawn error: ${err.message}`));
    });

    child.on("close", (exitCode) => {
      clearTimeout(timer);
      resolve({ stdout, stderr, exitCode, timedOut });
    });

    const stdinStream = child.stdin;
    if (stdinStream) {
      stdinStream.write(stdin);
      stdinStream.end();
    }
  });
}

function parseVerdict(stdout: string): Cvc5Verdict {
  const lines = stdout.split("\n").map((l) => l.trim()).filter((l) => l.length > 0);
  for (const line of lines) {
    if (line === "sat") return "sat";
    if (line === "unsat") return "unsat";
    if (line === "unknown") return "unknown";
  }
  return "unknown";
}

export function makeInvokeCvc5Stage(
  deps: MakeInvokeCvc5StageDeps = {},
): Stage<InvokeCvc5StageInput, InvokeCvc5StageOutput> {
  const producedBy = deps.producerVersion ?? "cvc5-symbolic@unknown";
  const spawnFn = deps.spawnFn ?? spawn;

  return {
    name: "invoke-cvc5",
    producedBy,

    serializeInput(input) {
      return {
        smtLib: input.smtLib,
        timeoutMs: input.timeoutMs ?? 30_000,
        binary: input.binary ?? "cvc5",
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as InvokeCvc5StageOutput;
    },

    async run(input) {
      const binary = input.binary ?? "cvc5";
      const timeoutMs = input.timeoutMs ?? 30_000;
      // CVC5 reads SMT-LIB from stdin when given `-` or via the
      // `--lang smt2` mode without a file argument. Modern CVC5 (1.0+)
      // accepts SMT-LIB on stdin by default; we pass `--lang=smt2` to
      // be explicit.
      const start = Date.now();
      const result = await runCvc5(
        spawnFn,
        binary,
        ["--lang=smt2", "--produce-models"],
        input.smtLib,
        timeoutMs,
      );
      const cvc5RunMs = Date.now() - start;

      if (result.timedOut) {
        return {
          cvc5Verdict: "timeout",
          stdout: result.stdout,
          stderr: result.stderr,
          cvc5RunMs,
        };
      }

      const cvc5Verdict = parseVerdict(result.stdout);
      return {
        cvc5Verdict,
        stdout: result.stdout,
        stderr: result.stderr,
        cvc5RunMs,
      };
    },
  };
}
