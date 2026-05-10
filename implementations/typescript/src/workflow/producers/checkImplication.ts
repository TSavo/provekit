/**
 * check-implication Stage: directional implication test between two
 * propertyHashes' SMT translations.
 *
 * The forensic core question for a library bump: when a propertyHash CID
 * changes (same human-readable name, different hash), is the new claim
 * logically stronger, weaker, equivalent, or incomparable to the old
 * claim? Answer mechanically by running two Z3 implication checks:
 *
 *   strongerThanOld := unsat(P_new AND NOT P_old)   // every model of P_new satisfies P_old
 *   weakerThanOld   := unsat(P_old AND NOT P_new)   // every model of P_old satisfies P_new
 *
 * Cross-tabulate:
 *
 *   stronger=true, weaker=true   → equivalent (same semantic content; usually a canonical-form refactor)
 *   stronger=true, weaker=false  → P_new STRENGTHENS P_old (acceptance set shrinks)
 *   stronger=false, weaker=true  → P_new WEAKENS P_old (acceptance set grows)
 *   stronger=false, weaker=false → INCOMPARABLE (each accepts inputs the other rejects)
 *
 * Either probe can return "unknown" (Z3 didn't decide within the budget).
 * The Stage surfaces the underlying verdicts so the caller can decide
 * how to handle undecidable corners: typically falling back to LLM
 * judgment OR surfacing "undecidable" as a load-bearing answer in the
 * forensic report ("the change crossed into a regime where mechanical
 * comparison fails").
 *
 * No LLM. The Stage's job is adjudication, not synthesis.
 *
 * Spec: protocol/specs/2026-04-29-the-semantic-envelope.md (case 4 routing
 * via mechanical implication; the forensic report's directionVerdict
 * field is this Stage's output).
 */

import { spawn } from "child_process";
import * as fs from "fs";
import * as path from "path";
import type { Stage } from "../types.js";

export const CHECK_IMPLICATION_CAPABILITY = "check-implication";

/**
 * A leaf solver entry: one binary that consumes some IR-compiled input
 * language, fully described by binary + compiler choice + flags.
 *
 * `compiler` names the IR translator that produces this solver's
 * input language. SMT solvers all share "smt-lib"; proof-assistant
 * entries would name "lean" / "coq". The framework dispatches the IR
 * through the named compiler before handing bytes to the binary.
 */
export interface SolverEntry {
  /** Display label (z3, cvc5, coq, ...). */
  type: string;
  /** Binary path or name. */
  binary: string;
  /** IR compiler. Default: "smt-lib", alternatives: "coq", "lean". */
  compiler: string;
  /** Argv template; {{TIMEOUT_S}} and {{TIMEOUT_MS}} substituted at runtime. */
  flags: string[];
  /** Per-probe timeout in ms. */
  timeoutMs: number;
}

/**
 * The framework's solver abstraction. Wraps one or more leaf entries.
 *
 * - One entry: that solver's verdict is the verdict.
 * - Multiple entries: all run in parallel; verdict = agreed value if all
 *   entries returned the same answer, else "unknown" (forensically the
 *   row is surfaced as a disagreement and the per-entry verdicts attach
 *   for transparency).
 *
 * The framework doesn't special-case N=1 vs N=many; it always calls
 * Solver.invoke() and gets back the unified verdict.
 */
export interface Solver {
  entries: SolverEntry[];
}

export interface CheckImplicationInput {
  /** SMT-LIB body of the OLD claim (declarations + assertion of the property; not the negation). */
  oldSmt: string;
  /** SMT-LIB body of the NEW claim. */
  newSmt: string;
  /** IR formula of the OLD claim (for Coq compiler). Optional - used when compiler is not smt-lib. */
  oldIr?: object;
  /** IR formula of the NEW claim (for Coq compiler). Optional - used when compiler is not smt-lib. */
  newIr?: object;
  /** The solver (one or more entries composed under agreement semantics). */
  solver: Solver;
}

export type ImplicationVerdict =
  | "equivalent"
  | "strengthened"
  | "weakened"
  | "incomparable"
  | "undecidable";

export type SolverProbeVerdict = "unsat" | "sat" | "unknown" | "timeout";

export interface PerSolverProbe {
  solverType: string;
  newImpliesOld: SolverProbeVerdict;
  oldImpliesNew: SolverProbeVerdict;
}

export interface CheckImplicationOutput {
  /** Composed final classification across all entries. */
  verdict: ImplicationVerdict;
  /** When the solver had multiple entries: per-entry probes for transparency. */
  perEntry: PerSolverProbe[];
  /** True iff every entry agreed on the same direction verdict. */
  allAgreed: boolean;
  /**
   * Convenience aliases that reflect the consensus probes (or the sole
   * entry's probes when N=1). Surfaced separately so simple readers can
   * ignore perEntry.
   */
  newImpliesOld: SolverProbeVerdict;
  oldImpliesNew: SolverProbeVerdict;
}

export interface MakeCheckImplicationStageDeps {
  producerVersion?: string;
}

export function makeCheckImplicationStage(
  deps: MakeCheckImplicationStageDeps = {},
): Stage<CheckImplicationInput, CheckImplicationOutput> {
  const producedBy = deps.producerVersion ?? "checkImplication@v1";

  return {
    name: "checkImplication",
    producedBy,

    serializeInput(input) {
      // Cache key includes the full solver composition; a row's verdict
      // depends on which entries voted on it. Order-independent: sort
      // entries by type so cache hits across config reorderings.
      const entries = [...input.solver.entries]
        .sort((a, b) => a.type.localeCompare(b.type))
        .map((e) => ({
          type: e.type,
          binary: e.binary,
          compiler: e.compiler,
          flags: e.flags,
          timeoutMs: e.timeoutMs,
        }));
      return {
        oldSmt: input.oldSmt,
        newSmt: input.newSmt,
        solverEntries: entries,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as CheckImplicationOutput;
    },

    async run(input) {
      // For each entry, compile the implication probes via that entry's
      // configured compiler. SMT-LIB-class entries use smt-lib compiler;
      // Coq uses coq compiler with .v file + coqc.
      
      const smtSolvers = input.solver.entries.filter(e => e.compiler === "smt-lib");
      const coqSolvers = input.solver.entries.filter(e => e.compiler === "coq");
      
      const probeAB = wrapImplicationProbe(input.newSmt, input.oldSmt);
      const probeBA = wrapImplicationProbe(input.oldSmt, input.newSmt);
      // Process SMT-LIB solvers
      const smtResults: Array<{ solverType: string; newImpliesOld: SolverProbeVerdict; oldImpliesNew: SolverProbeVerdict; verdict: ImplicationVerdict }> = smtSolvers.length > 0 
        ? await Promise.all(
            smtSolvers.map(async (entry) => {
              const [ab, ba] = await Promise.all([
                invokeSolver(entry, probeAB),
                invokeSolver(entry, probeBA),
              ]);
              return {
                solverType: entry.type,
                newImpliesOld: ab,
                oldImpliesNew: ba,
                verdict: classifyVerdict(ab, ba),
              };
            }),
          )
        : [];

      // Process Coq solvers (if any)
      let coqResults: Array<{ solverType: string; newImpliesOld: SolverProbeVerdict; oldImpliesNew: SolverProbeVerdict; verdict: ImplicationVerdict }> = [];
      
      if (coqSolvers.length > 0) {
        // Check if we have IR formulas for Coq
        if (!input.newIr || !input.oldIr) {
          console.warn("checkImplication: Coq solvers configured but no IR provided - skipping Coq");
        } else {
          // Process each Coq solver with IR
          coqResults = await Promise.all(
            coqSolvers.map(async (entry) => {
              // For implication, we need to check both directions with Coq
              // Coq verification: if coqc succeeds (exit 0), it means "proved" (like unsat)
              const newImpliesOld = await invokeCoqSolver(entry, input.newIr as object);
              const oldImpliesNew = await invokeCoqSolver(entry, input.oldIr as object);
              
              // For Coq: unsat (proved) means implication holds
              // sat (failed to prove) means implication doesn't hold
              return {
                solverType: entry.type,
                newImpliesOld,
                oldImpliesNew,
                verdict: classifyVerdict(newImpliesOld, oldImpliesNew),
              };
            }),
          );
        }
      }
      
      const entryResults: Array<{ solverType: string; newImpliesOld: SolverProbeVerdict; oldImpliesNew: SolverProbeVerdict; verdict: ImplicationVerdict }> = [...smtResults, ...coqResults];
      
      if (entryResults.length === 0) {
        throw new Error(
          `checkImplication: no usable solvers configured. ` +
          `SMT-LIB solvers require smt-lib compiler, Coq requires coq compiler.`,
        );
      }

      // Consensus: all entries agree on the same final verdict, or the
      // composed answer collapses to "undecidable" + the per-entry detail
      // surfaces the disagreement.
      const verdicts = entryResults.map((r: { verdict: ImplicationVerdict }) => r.verdict);
      const allAgreed = verdicts.every((v: ImplicationVerdict) => v === verdicts[0]);
      const verdict: ImplicationVerdict = allAgreed ? verdicts[0]! : "undecidable";

      // For the convenience aliases: when agreed, surface the consensus
      // probes; when disagreed, surface the first entry's probes (the
      // perEntry array holds the full picture).
      const head = entryResults[0]!;
      return {
        verdict,
        perEntry: entryResults.map((r: { solverType: string; newImpliesOld: SolverProbeVerdict; oldImpliesNew: SolverProbeVerdict }) => ({
          solverType: r.solverType,
          newImpliesOld: r.newImpliesOld,
          oldImpliesNew: r.oldImpliesNew,
        })),
        allAgreed,
        newImpliesOld: head.newImpliesOld,
        oldImpliesNew: head.oldImpliesNew,
      };
    },
  };
}

/**
 * Build a single-shot SMT-LIB probe whose check-sat answers "is P AND
 * NOT Q satisfiable?" The caller passes raw SMT-LIB bodies for P and Q
 * (declarations + assertions). We assemble:
 *
 *   <declarations from P>
 *   <declarations from Q>
 *   (assert <combined assertion of P>)
 *   (assert (not <combined assertion of Q>))
 *   (check-sat)
 *
 * This is a v1 surgery: we lift only the declare-* lines out of each
 * input and treat everything else as the assertion. A more robust impl
 * would parse SMT-LIB and combine ASTs, but for properties already in
 * canonical form (declarations at the top, assertions at the bottom)
 * the line-based partition works.
 */
function wrapImplicationProbe(p: string, q: string): string {
  const pParts = splitSmt(p);
  const qParts = splitSmt(q);
  return [
    "(set-logic ALL)",
    pParts.declarations.join("\n"),
    // Skip Q's declare-const lines that re-declare P's variables; SMT
    // requires unique declarations. This is a v1 conservative approach:
    // collect declared names from P, skip any line in Q whose first
    // tokens are `(declare-const <name>` or `(declare-fun <name>` if
    // <name> is already declared by P.
    qParts.declarations.filter((line) => !pParts.declaredNames.has(extractDeclaredName(line))).join("\n"),
    pParts.assertions.join("\n"),
    `(assert (not (and ${qParts.assertions.map(stripAssertWrapper).join(" ")})))`,
    "(check-sat)",
  ].join("\n");
}

function splitSmt(smt: string): {
  declarations: string[];
  assertions: string[];
  declaredNames: Set<string>;
} {
  const declarations: string[] = [];
  const assertions: string[] = [];
  const declaredNames = new Set<string>();
  for (const rawLine of smt.split("\n")) {
    const line = rawLine.trim();
    if (line.length === 0) continue;
    if (line.startsWith("(declare-const") || line.startsWith("(declare-fun") || line.startsWith("(declare-sort")) {
      declarations.push(line);
      const name = extractDeclaredName(line);
      if (name) declaredNames.add(name);
    } else if (line.startsWith("(assert")) {
      assertions.push(line);
    } else if (line.startsWith("(set-logic") || line.startsWith("(check-sat") || line.startsWith("(get-model") || line.startsWith(";")) {
      // Skip; we add our own logic + check-sat.
    } else if (line.length > 0) {
      // Treat unrecognized non-empty lines as part of the previous assertion (continuation).
      if (assertions.length > 0) {
        assertions[assertions.length - 1] += " " + line;
      } else if (declarations.length > 0) {
        declarations[declarations.length - 1] += " " + line;
      }
    }
  }
  return { declarations, assertions, declaredNames };
}

function extractDeclaredName(line: string): string {
  const match = line.match(/^\(declare-(?:const|fun|sort)\s+([A-Za-z_][\w-]*)/);
  return match?.[1] ?? "";
}

function stripAssertWrapper(line: string): string {
  // "(assert FOO)" → "FOO"; we tolerate trailing whitespace and nested parens.
  const m = line.match(/^\(assert\s+(.*)\)\s*$/s);
  return m ? m[1] : line;
}

function classifyVerdict(
  ab: SolverProbeVerdict,
  ba: SolverProbeVerdict,
): ImplicationVerdict {
  if (ab === "unknown" || ab === "timeout" || ba === "unknown" || ba === "timeout") return "undecidable";
  if (ab === "unsat" && ba === "unsat") return "equivalent";
  if (ab === "unsat" && ba === "sat") return "strengthened";
  if (ab === "sat" && ba === "unsat") return "weakened";
  return "incomparable";
}

/**
 * Lazy singleton for the z3-solver WebAssembly module. We prefer WASM
 * because it eliminates the system-binary dependency that breaks CI
 * when `z3` is not on $PATH. The init Promise is cached so every
 * solver invocation shares a single wasm compile+instantiation.
 */
let _z3WasmReady: Promise<any> | null = null;

function getZ3Wasm(): Promise<any> {
  if (!_z3WasmReady) {
    _z3WasmReady = (async () => {
      try {
        const z3 = require("z3-solver");
        const api = await z3.init();
        return api;
      } catch (e) {
        _z3WasmReady = null; // reset on failure so next call can retry
        throw e;
      }
    })();
  }
  return _z3WasmReady;
}

/**
 * Feed an SMT-LIB2 script to the WASM z3-solver and return its
 * check-sat verdict. No system binary required.
 */
async function invokeSolverWasm(
  script: string,
  timeoutMs: number,
): Promise<"sat" | "unsat" | "unknown" | "timeout"> {
  try {
    const api = await getZ3Wasm();
    const ctx = new api.Context("main");
    const solver = new ctx.Solver();
    if (timeoutMs > 0) solver.set("timeout", timeoutMs);
    solver.fromString(script);
    const result = await solver.check();
    if (result === "sat" || result === "unsat") return result;
    return "unknown";
  } catch {
    return "unknown";
  }
}

/**
 * Solver-agnostic invocation. The SolverEntry describes everything
 * needed to run any SMT-LIB-2.6-conformant solver: binary + flags with
 * {{TIMEOUT_S}} and {{TIMEOUT_MS}} placeholders. Adding a new solver
 * (Bitwuzla, Boolector, MathSAT, …) is a YAML edit, not a TS edit.
 *
 * For smt-lib entries the WASM-based z3-solver is tried first (no
 * system binary required). If WASM fails, we fall back to the solver
 * pool (long-lived processes) to avoid per-invocation spawn overhead.
 */
export async function invokeSolver(
  solver: SolverEntry,
  script: string,
): Promise<"sat" | "unsat" | "unknown" | "timeout"> {
  // Primary path: WASM z3-solver (works without a system binary)
  if (solver.compiler === "smt-lib") {
    try {
      return await invokeSolverWasm(script, solver.timeoutMs);
    } catch {
      // Fall through to pool-based fallback
    }
  }

  // Fallback: use solver pool for system binary
  return await invokeSolverViaPool(solver, script);
}

/**
 * Invoke a solver via the long-lived worker pool. Extracts declarations
 * from the script and uses (push)/(pop) for per-query isolation.
 */
async function invokeSolverViaPool(
  solver: SolverEntry,
  script: string,
): Promise<"sat" | "unsat" | "unknown" | "timeout"> {
  // Late import to avoid circular dependency at module load time
  const { getGlobalPool } = await import("../../test-support/smtPool.js");
  const pool = await getGlobalPool();
  const worker = await pool.acquire();

  try {
    worker.push();

    // Send each line of the script (declarations + assertions)
    for (const line of script.split("\n")) {
      const trimmed = line.trim();
      if (trimmed.length === 0 || trimmed.startsWith(";")) continue;

      if (trimmed.startsWith("(declare-") || trimmed.startsWith("(assert")) {
        worker.assert(trimmed);
      } else if (trimmed.startsWith("(check-sat")) {
        // Skip; we'll call checkSat() explicitly
      } else if (!trimmed.startsWith("(set-logic")) {
        // Other commands: skip (set-logic will be implicit in pool)
      }
    }

    const result = await worker.checkSat(solver.timeoutMs);
    return result;
  } catch {
    return "unknown";
  } finally {
    worker.pop();
    worker.release();
  }
}

/**
 * Invoke Coq solver: compile IR to Coq using the Rust compiler binary,
 * then run coqc to verify the proof.
 * 
 * For Coq, we need a different approach than SMT-LIB:
 * 1. Compile IR to Coq .v file using provekit-ir-coq
 * 2. Run coqc on the .v file
 * 3. Exit code 0 = proven (unsat), non-zero = failed (sat/unknown)
 */
export async function invokeCoqSolver(
  solver: SolverEntry,
  irFormula: object,
): Promise<"unsat" | "sat" | "unknown" | "timeout"> {
  const timeoutMs = solver.timeoutMs;
  const coqBinary = solver.binary; // e.g., "coqc"
  const compilerBinary = solver.binary.replace("coqc", "provekit-ir-coq");
  
  return new Promise((resolve) => {
    // Step 1: Compile IR to Coq using Rust binary
    const compile = spawn(compilerBinary, [], { stdio: ["pipe", "pipe", "pipe"] });
    
    let coqCode = "";
    compile.stdout?.on("data", (c) => (coqCode += c.toString()));
    compile.stderr?.on("data", () => { /* discard compile errors */ });
    
    compile.on("error", () => { resolve("unknown"); });
    compile.on("close", (code) => {
      if (code !== 0 || !coqCode.trim()) {
        resolve("unknown");
        return;
      }
      
      // Step 2: Write to temp file and run coqc
      const tmpFile = path.join("/tmp", `provekit_coq_${Date.now()}.v`);
      fs.writeFileSync(tmpFile, coqCode);
      
      const verify = spawn(coqBinary, [tmpFile], { stdio: ["pipe", "pipe", "pipe"] });
      
      const timer = setTimeout(() => {
        try { verify.kill("SIGKILL"); } catch { /* ignore */ }
        resolve("timeout");
      }, timeoutMs + 250);
      
      verify.on("error", () => { clearTimeout(timer); resolve("unknown"); });
      verify.on("close", (exitCode) => {
        clearTimeout(timer);
        // Clean up temp file
        try { fs.unlinkSync(tmpFile); } catch { /* ignore */ }
        
        // Coq: exit code 0 = proof verified (like "unsat")
        // non-zero = failed (like "sat" or "unknown")
        if (exitCode === 0) {
          resolve("unsat"); // proof succeeded = no counterexample
        } else {
          resolve("sat"); // proof failed = counterexample exists
        }
      });
    });
    
    if (compile.stdin) {
      compile.stdin.write(JSON.stringify(irFormula));
      compile.stdin.end();
    }
  });
}
