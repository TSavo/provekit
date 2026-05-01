/**
 * provekit.config.yaml — per-project provider configuration.
 *
 * Same shape as tsconfig.json / vite.config.ts: ONE invocation entry
 * point per workflow, project-level config picks the implementations.
 * The framework's canonical workflows (`provekit diff`, `provekit must`,
 * `provekit verify`) reference ABSTRACT capabilities; this config names
 * the concrete providers that satisfy them.
 *
 * Provider lists are arrays. One entry = run that provider. Multiple
 * entries = run all in parallel, agreement is the verdict. There's no
 * special case for "two solvers" or "both" — the array IS the choice.
 *
 * Schema (v1):
 *
 *   providers:
 *     solver:
 *       - type: z3
 *         timeoutMs: 5000
 *       - type: cvc5
 *         timeoutMs: 5000
 *
 * One entry → that solver runs. Multiple entries → all run in parallel,
 * verdict is the agreed value across solvers; per-solver verdicts are
 * attached for forensic transparency. Same primitive will eventually
 * apply to providers.llm, providers.prover, etc.
 *
 * No mandatory config: a project without `provekit.config.yaml` gets
 * the defaults (one solver, Z3, 5000ms timeout). Zero-config works out
 * of the box; the file is opt-in for swapping or composing providers.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { parse as parseYaml } from "yaml";

/**
 * A leaf solver entry. Free-form `type` is a label for forensic rows.
 * The actual invocation is fully described by `compiler` + `binary` +
 * `flags`.
 *
 * `compiler` names the IR translator that produces this solver's input
 * language. SMT solvers (Z3, CVC5, MathSAT, Bitwuzla, Boolector, Yices)
 * all share `compiler: smt-lib`. Proof assistants name a per-system
 * compiler (`compiler: lean`, etc.). Adding an SMT solver is one new
 * YAML entry; adding a new solver class is one entry plus one compiler
 * implementation.
 *
 * `flags` is a template — strings may contain `{{TIMEOUT_S}}` and
 * `{{TIMEOUT_MS}}` placeholders that the runtime substitutes. The
 * compiled script is delivered on stdin; verdict is read from the
 * last non-empty line of stdout (sat / unsat / unknown for SMT-LIB).
 */
export interface SolverEntry {
  /** Display label for forensic output (e.g. "z3", "cvc5", "coq"). */
  type: string;
  /** Path or name of the binary (default: same as `type`). */
  binary?: string;
  /**
   * IR compiler that produces this solver's input language. Default:
   * "smt-lib" (covers Z3, CVC5, MathSAT, Bitwuzla, Boolector, Yices).
   * Alternatives: "coq", "lean".
   */
  compiler?: string;
  /**
   * Dialect for the compiler (e.g., "smt-lib-v2.6", "coq"). 
   * Passed to the compiler to select the output format.
   */
  dialect?: string;
  /** Argv flags template; supports {{TIMEOUT_S}} and {{TIMEOUT_MS}}. */
  flags: string[];
  /** Per-probe timeout in ms. */
  timeoutMs: number;
}

export interface ProvekitConfig {
  providers: {
    /**
     * Solvers to run. One entry → that solver. Multiple entries → all
     * run in parallel, verdict is the agreed value.
     */
    solver: SolverEntry[];
  };
}

const DEFAULT_CONFIG: ProvekitConfig = {
  providers: {
    solver: [
      {
        type: "z3",
        binary: "z3",
        compiler: "smt-lib",
        flags: ["-in", "-T:{{TIMEOUT_S}}"],
        timeoutMs: 5000,
      },
    ],
  },
};

/**
 * Load + parse provekit.config.yaml at the project root. Returns
 * defaults when the file is missing or malformed (with a stderr warning
 * for malformed cases — silent ignore would hide config typos).
 */
export function loadProvekitConfig(projectRoot: string): ProvekitConfig {
  const path = join(projectRoot, "provekit.config.yaml");
  if (!existsSync(path)) return DEFAULT_CONFIG;
  let raw: unknown;
  try {
    raw = parseYaml(readFileSync(path, "utf-8"));
  } catch (err) {
    process.stderr.write(`[provekit] warning: failed to parse ${path}: ${(err as Error).message}\n`);
    return DEFAULT_CONFIG;
  }
  return mergeWithDefaults(raw);
}

/**
 * Resolve the solver entries from a loaded ProvekitConfig. Returns the
 * array as-is — workflows iterate it; one entry runs once, multiple
 * entries run in parallel.
 */
export function resolveSolverEntries(config: ProvekitConfig): SolverEntry[] {
  return config.providers.solver;
}

function mergeWithDefaults(raw: unknown): ProvekitConfig {
  const root = (raw && typeof raw === "object") ? raw as Record<string, unknown> : {};
  const providers = (root.providers && typeof root.providers === "object")
    ? root.providers as Record<string, unknown>
    : {};
  const solverRaw = providers.solver;
  const entries: SolverEntry[] = [];
  if (Array.isArray(solverRaw)) {
    for (const entryRaw of solverRaw) {
      if (!entryRaw || typeof entryRaw !== "object") continue;
      const e = entryRaw as Record<string, unknown>;
      if (typeof e.type !== "string" || e.type.length === 0) continue;
      // Flags array is required for new solvers; if missing for known
      // legacy types (z3 / cvc5), fall back to canonical defaults so
      // existing configs that don't specify flags keep working.
      let flags: string[] | null = null;
      if (Array.isArray(e.flags) && e.flags.every((f) => typeof f === "string")) {
        flags = e.flags as string[];
      } else if (e.type === "z3") {
        flags = ["-in", "-T:{{TIMEOUT_S}}"];
      } else if (e.type === "cvc5") {
        flags = ["--lang=smt2", "--tlimit-per={{TIMEOUT_MS}}"];
      }
      if (flags === null) continue;
      entries.push({
        type: e.type,
        binary: typeof e.binary === "string" ? e.binary : e.type,
        compiler: typeof e.compiler === "string" ? e.compiler : "smt-lib",
        dialect: typeof e.dialect === "string" ? e.dialect : undefined,
        flags,
        timeoutMs: typeof e.timeoutMs === "number" ? e.timeoutMs : 5000,
      });
    }
  }
  return {
    providers: {
      solver: entries.length > 0 ? entries : DEFAULT_CONFIG.providers.solver,
    },
  };
}
