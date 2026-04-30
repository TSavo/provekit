/**
 * provideLeanProof Action — invoke Lean 4 on a theorem statement plus an
 * externally-supplied proof.
 *
 * Per the team-lead spec (2026-04-29 cross-paradigm composition test):
 * Action because subprocess spawn is side-effecting, and the proof file
 * is supplied by the caller from outside the IR pipeline (so the input
 * isn't fully captured by the IR CID).
 *
 * Architectural note (surfaced for follow-up): `invokeZ3` is a Stage
 * despite also spawning a subprocess, on the reasoning that
 * "identical SMT-LIB + identical z3 version produces the identical
 * verdict." Lean checking a fixed theorem source + proof + version is
 * equally deterministic. Treating one as Stage and the other as Action
 * may break the cross-paradigm symmetry the architectural claim asserts.
 * Following the team-lead spec as written; the Stage-vs-Action question
 * is flagged in the final report rather than silently switched.
 *
 * Input (replacement-shaped — proof is external):
 *   - theoremSource: Lean source for the theorem statement (with sorry).
 *   - proofText: the proof body the user wrote, replacing `sorry`.
 *   - theoremName: identifier to verify the source declares.
 *   - timeoutMs / binary: spawn ergonomics.
 *
 * Behavior:
 *   - Concatenates source (with `sorry` swapped for proofText) into a
 *     single .lean file in a tmp dir.
 *   - Runs `lean --json <file>` with stdin closed.
 *   - Verdict mapping:
 *       exit code 0 + no errors  -> "valid"
 *       exit code != 0 OR errors -> "invalid"
 *       timeout                  -> "timeout"
 *   - Returns the verdict, raw stdout/stderr, and the lean version best-
 *     effort detected.
 *
 * The Action's audit memento records the inputs and the verdict; the
 * verdict-memento Action (mintLeanVerdictMemento) writes the verdict
 * memento at the original (bindingHash, propertyHash) the locate-memento
 * Stage recovered.
 */

import { spawn, spawnSync } from "child_process";
import { writeFileSync, mkdtempSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import type { Action } from "../types.js";

export const PROVIDE_LEAN_PROOF_CAPABILITY = "provide-lean-proof";

export type LeanVerdict = "valid" | "invalid" | "timeout" | "error";

export interface ProvideLeanProofActionInput {
  /** Lean source for the theorem statement. May contain `sorry` placeholder. */
  theoremSource: string;
  /** The proof body to splice in for `sorry`. Lean tactic block or term. */
  proofText: string;
  /** Theorem identifier. Used for error reporting and source validation. */
  theoremName: string;
  /** Solver timeout in milliseconds. Default 60_000. */
  timeoutMs?: number;
  /** Override the lean binary path. Default "lean" (PATH lookup). */
  binary?: string;
}

export interface ProvideLeanProofResource {
  /** Lean's verdict on the combined source + proof. */
  verdict: LeanVerdict;
  /** Combined .lean source written to tmp file (source with proof spliced in). */
  combinedSource: string;
  /** Raw stdout from lean. */
  stdout: string;
  /** Raw stderr from lean. */
  stderr: string;
  /** Wall-clock duration of the lean invocation. */
  leanRunMs: number;
  /** Lean version (best-effort) — undefined if `lean --version` not detected. */
  leanVersion?: string;
}

export interface MakeProvideLeanProofActionDeps {
  /** Override producer identity. Default detected via `lean --version`. */
  producerVersion?: string;
  /** Spawn function override. Test-only seam. */
  spawnFn?: typeof spawn;
}

export class ProvideLeanProofError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ProvideLeanProofError";
  }
}

interface RunResult {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
}

function runLean(
  spawnFn: typeof spawn,
  binary: string,
  filePath: string,
  timeoutMs: number,
): Promise<RunResult> {
  return new Promise((resolve, reject) => {
    let child;
    try {
      child = spawnFn(binary, ["--json", filePath], {
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (err) {
      reject(
        new ProvideLeanProofError(
          `failed to spawn lean (${binary}): ${(err as Error).message} — install lean to use the prove-with-lean workflow`,
        ),
      );
      return;
    }

    let stdout = "";
    let stderr = "";
    let timedOut = false;
    if (child.stdout) child.stdout.on("data", (chunk) => (stdout += chunk.toString()));
    if (child.stderr) child.stderr.on("data", (chunk) => (stderr += chunk.toString()));

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
          new ProvideLeanProofError(
            `lean binary not found in PATH (${binary}) — install lean to use the prove-with-lean workflow`,
          ),
        );
        return;
      }
      reject(new ProvideLeanProofError(`lean spawn error: ${err.message}`));
    });

    child.on("close", (exitCode) => {
      clearTimeout(timer);
      resolve({ stdout, stderr, exitCode, timedOut });
    });
  });
}

/**
 * Detect lean version best-effort via `lean --version`. Synchronous so we
 * can do it once at Action construction. Returns undefined if lean isn't
 * available.
 */
export function detectLeanVersion(binary: string = "lean"): string | undefined {
  try {
    const result = spawnSync(binary, ["--version"], { encoding: "utf-8" });
    if (result.status !== 0) return undefined;
    const text = (result.stdout + result.stderr).trim();
    const match = text.match(/Lean[^\d]*(\d+\.\d+\.\d+(?:-[\w.]+)?)/);
    return match ? match[1] : text.split("\n")[0]?.trim() || undefined;
  } catch {
    return undefined;
  }
}

/**
 * Splice the user's proof into the theorem statement, replacing the
 * `sorry` placeholder. The translator's `emitLeanTheorem` always emits
 * `:= by\n  sorry` as the proof position. We replace exactly that suffix.
 */
function spliceProof(theoremSource: string, proofText: string): string {
  const sorryPattern = /:= by\n\s+sorry\s*$/;
  if (!sorryPattern.test(theoremSource.trimEnd() + "\n")) {
    // Tolerate either "sorry" alone or "by ... sorry"; replace last "sorry".
    const lastSorry = theoremSource.lastIndexOf("sorry");
    if (lastSorry < 0) {
      throw new ProvideLeanProofError(
        "theorem source did not contain `sorry` placeholder — cannot splice proof",
      );
    }
    return theoremSource.slice(0, lastSorry) + proofText + theoremSource.slice(lastSorry + "sorry".length);
  }
  return theoremSource.replace(sorryPattern, `:= ${proofText}\n`);
}

/**
 * Parse lean's output to determine whether the theorem checked. Lean emits
 * JSON-line diagnostics with `--json`; "error"-severity messages indicate
 * the proof did not check. Exit code 0 + no error messages = valid.
 */
function parseVerdict(
  exitCode: number | null,
  stdout: string,
  stderr: string,
  timedOut: boolean,
): LeanVerdict {
  if (timedOut) return "timeout";
  if (exitCode === 0) {
    // Even with exit 0, --json may emit error diagnostics on stdout.
    // Check both streams for an `"severity":"error"` token.
    const combined = stdout + stderr;
    if (/"severity"\s*:\s*"error"/.test(combined)) return "invalid";
    if (/\berror\b/i.test(stderr) && stderr.length > 0) return "invalid";
    return "valid";
  }
  if (exitCode === null) return "error";
  return "invalid";
}

export function makeProvideLeanProofAction(
  deps: MakeProvideLeanProofActionDeps = {},
): Action<ProvideLeanProofActionInput, ProvideLeanProofResource> {
  const detectedVersion = detectLeanVersion();
  const producedBy =
    deps.producerVersion ??
    (detectedVersion ? `lean@${detectedVersion}` : "lean@unknown");
  const spawnFn = deps.spawnFn ?? spawn;

  return {
    name: "provideLeanProof",
    producedBy,

    serializeInput(input) {
      return {
        theoremSource: input.theoremSource,
        proofText: input.proofText,
        theoremName: input.theoremName,
        timeoutMs: input.timeoutMs ?? 60_000,
        binary: input.binary ?? "lean",
      };
    },

    describeResource(resource) {
      const ver = resource.leanVersion ? ` (lean ${resource.leanVersion})` : "";
      return `lean verdict ${resource.verdict}${ver} in ${resource.leanRunMs}ms`;
    },

    async run(input) {
      const binary = input.binary ?? "lean";
      const timeoutMs = input.timeoutMs ?? 60_000;
      const combinedSource = spliceProof(input.theoremSource, input.proofText);

      const dir = mkdtempSync(join(tmpdir(), "provekit-lean-"));
      const filePath = join(dir, `${input.theoremName}.lean`);
      writeFileSync(filePath, combinedSource, "utf-8");

      try {
        const start = Date.now();
        const result = await runLean(spawnFn, binary, filePath, timeoutMs);
        const leanRunMs = Date.now() - start;
        const verdict = parseVerdict(
          result.exitCode,
          result.stdout,
          result.stderr,
          result.timedOut,
        );

        const out: ProvideLeanProofResource = {
          verdict,
          combinedSource,
          stdout: result.stdout,
          stderr: result.stderr,
          leanRunMs,
        };
        if (detectedVersion !== undefined) {
          out.leanVersion = detectedVersion;
        }
        return out;
      } finally {
        try {
          rmSync(dir, { recursive: true, force: true });
        } catch {
          // best-effort cleanup
        }
      }
    },
  };
}
