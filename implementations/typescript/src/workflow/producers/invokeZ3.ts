/**
 * invoke-z3 Stage — refute workflow's solver invocation step.
 *
 * Spawns `z3 -in -smt2`, writes SMT-LIB to stdin, parses sat/unsat, and
 * extracts a model on sat. Stage rather than Action because the verdict
 * is a content-addressable claim about a deterministic input: identical
 * SMT-LIB + identical z3 version produces the identical verdict, so it
 * caches.
 *
 * Verdict mapping (refute semantics — the assertion is the property,
 * the SMT-LIB asserts the NEGATION for unsat-check):
 *
 *   z3 returns      | property verdict | meaning
 *   ----------------+------------------+--------------------------------
 *   unsat           | holds            | no counterexample exists
 *   sat             | violated         | model is the counterexample
 *   timeout / unkn. | undecidable      | solver could not resolve
 *
 * The Stage's Stage-memento verdict (written by runStage) is always
 * "holds" — it claims "z3 ran on this input and produced this output."
 * The PROPERTY verdict (holds / violated / undecidable) lives inside
 * the Stage's output and is materialized as a separate verdict memento
 * by the mint-verdict-memento Action.
 *
 * If z3 isn't installed, run() throws with a clear message. The error
 * mentions `z3` so consumers know to install it.
 *
 * Caching note: timeoutMs is part of serializeInput so different
 * timeouts get different cache slots. producedBy includes the z3
 * version (best-effort) so version bumps invalidate.
 */

import { spawn } from "child_process";
import { parseZ3Model } from "../../z3/modelParser.js";
import type { Z3Value } from "../../z3/modelParser.js";
import type { Stage } from "../types.js";

export const INVOKE_Z3_CAPABILITY = "invoke-z3";

export type Z3Verdict = "sat" | "unsat" | "unknown" | "timeout";

export interface InvokeZ3StageInput {
  smtLib: string;
  /** Solver timeout in milliseconds. Default 30_000. */
  timeoutMs?: number;
  /** Override the z3 binary path. Default: "z3" (PATH lookup). */
  binary?: string;
}

/**
 * JSON-safe wire form of a Z3 model assignment. Identical to Z3Value
 * except `Int` values use `bigintString` (decimal text) instead of a
 * native `bigint`, so the output round-trips through JSON.stringify
 * without a bigint codec. Downstream consumers that need a real
 * `bigint` re-parse via `BigInt(value.bigintString)`.
 */
export type Z3WireValue =
  | { sort: "Real"; value: number | "div_by_zero" | "nan" | "+infinity" | "-infinity" }
  | { sort: "Int"; bigintString: string }
  | { sort: "Bool"; value: boolean }
  | { sort: "String"; value: string }
  | { sort: "Other"; raw: string };

export interface InvokeZ3StageOutput {
  z3Verdict: Z3Verdict;
  /** Raw stdout from z3. Empty string on timeout. */
  stdout: string;
  /** Raw stderr from z3. */
  stderr: string;
  /** Wall-clock duration of the z3 invocation. */
  z3RunMs: number;
  /**
   * Counterexample assignments parsed from z3's `(get-model)`. Present
   * only on sat. Keys are the SMT-LIB names; values are the JSON-safe
   * Z3WireValue records (Int sorts carry decimal text, not native bigint).
   */
  counterexample?: Record<string, Z3WireValue>;
}

function toWireValue(v: Z3Value): Z3WireValue {
  if (v.sort === "Int") {
    return { sort: "Int", bigintString: v.value.toString() };
  }
  return v;
}

export interface MakeInvokeZ3StageDeps {
  /** Override producer identity. Default: detected via `z3 --version`. */
  producerVersion?: string;
  /**
   * Spawn function override. Test-only seam — production passes the real
   * `child_process.spawn`.
   */
  spawnFn?: typeof spawn;
}

export class InvokeZ3Error extends Error {
  constructor(message: string) {
    super(message);
    this.name = "InvokeZ3Error";
  }
}

interface RunResult {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
}

function runZ3(
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
        new InvokeZ3Error(
          `failed to spawn z3 (${binary}): ${(err as Error).message} — install z3 to use the refute workflow`,
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
          new InvokeZ3Error(
            `z3 binary not found in PATH (${binary}) — install z3 to use the refute workflow`,
          ),
        );
        return;
      }
      reject(new InvokeZ3Error(`z3 spawn error: ${err.message}`));
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

function parseVerdict(stdout: string): Z3Verdict {
  const lines = stdout.split("\n").map((l) => l.trim()).filter((l) => l.length > 0);
  for (const line of lines) {
    if (line === "sat") return "sat";
    if (line === "unsat") return "unsat";
    if (line === "unknown") return "unknown";
  }
  return "unknown";
}

function extractModelBlock(stdout: string): string | null {
  // `(get-model)` returns a parenthesized block; we look for the first
  // top-level "(" after the verdict line. Z3 outputs the verdict on its
  // own line, then the model on subsequent lines.
  const idx = stdout.indexOf("(");
  if (idx < 0) return null;
  // Find balanced matching ")".
  let depth = 0;
  for (let i = idx; i < stdout.length; i++) {
    const c = stdout[i];
    if (c === "(") depth++;
    else if (c === ")") {
      depth--;
      if (depth === 0) {
        return stdout.slice(idx, i + 1);
      }
    }
  }
  return null;
}

export function makeInvokeZ3Stage(
  deps: MakeInvokeZ3StageDeps = {},
): Stage<InvokeZ3StageInput, InvokeZ3StageOutput> {
  const producedBy = deps.producerVersion ?? "z3-symbolic@unknown";
  const spawnFn = deps.spawnFn ?? spawn;

  return {
    name: "invoke-z3",
    producedBy,

    serializeInput(input) {
      return {
        smtLib: input.smtLib,
        timeoutMs: input.timeoutMs ?? 30_000,
        binary: input.binary ?? "z3",
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as InvokeZ3StageOutput;
    },

    async run(input) {
      const binary = input.binary ?? "z3";
      const timeoutMs = input.timeoutMs ?? 30_000;
      // Append (get-model) so we can extract counterexamples on sat.
      // Z3 ignores (get-model) after unsat with an error on stderr,
      // which we tolerate.
      const script = input.smtLib.endsWith("\n")
        ? input.smtLib + "(get-model)\n"
        : input.smtLib + "\n(get-model)\n";

      const start = Date.now();
      const result = await runZ3(
        spawnFn,
        binary,
        ["-in", "-smt2"],
        script,
        timeoutMs,
      );
      const z3RunMs = Date.now() - start;

      if (result.timedOut) {
        return {
          z3Verdict: "timeout",
          stdout: result.stdout,
          stderr: result.stderr,
          z3RunMs,
        };
      }

      const z3Verdict = parseVerdict(result.stdout);
      const out: InvokeZ3StageOutput = {
        z3Verdict,
        stdout: result.stdout,
        stderr: result.stderr,
        z3RunMs,
      };
      if (z3Verdict === "sat") {
        const block = extractModelBlock(
          result.stdout.slice(result.stdout.indexOf("sat") + 3),
        );
        if (block) {
          const model = parseZ3Model(block);
          if (model.size > 0) {
            const wire: Record<string, Z3WireValue> = {};
            for (const [k, v] of model) wire[k] = toWireValue(v);
            out.counterexample = wire;
          }
        }
      }
      return out;
    },
  };
}
