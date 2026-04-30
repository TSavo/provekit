/**
 * check-implication Stage — directional implication test between two
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
 * how to handle undecidable corners — typically falling back to LLM
 * judgment OR surfacing "undecidable" as a load-bearing answer in the
 * forensic report ("the change crossed into a regime where mechanical
 * comparison fails").
 *
 * No LLM. The Stage's job is adjudication, not synthesis.
 *
 * Spec: docs/specs/2026-04-29-the-semantic-envelope.md (case 4 routing
 * via mechanical implication; the forensic report's directionVerdict
 * field is this Stage's output).
 */

import { spawn } from "child_process";
import type { Stage } from "../types.js";

export const CHECK_IMPLICATION_CAPABILITY = "check-implication";

export interface CheckImplicationInput {
  /** SMT-LIB body of the OLD claim (declarations + assertion of the property; not the negation). */
  oldSmt: string;
  /** SMT-LIB body of the NEW claim. */
  newSmt: string;
  /** Optional Z3 binary path. Default: "z3". */
  z3Binary?: string;
  /** Per-probe timeout in ms. Default: 5000. */
  timeoutMs?: number;
}

export type ImplicationVerdict =
  | "equivalent"
  | "strengthened"
  | "weakened"
  | "incomparable"
  | "undecidable";

export interface CheckImplicationOutput {
  /** Final classification. */
  verdict: ImplicationVerdict;
  /** Z3 verdict on (P_new ∧ ¬P_old). unsat means P_new ⊨ P_old. */
  newImpliesOld: "unsat" | "sat" | "unknown" | "timeout";
  /** Z3 verdict on (P_old ∧ ¬P_new). unsat means P_old ⊨ P_new. */
  oldImpliesNew: "unsat" | "sat" | "unknown" | "timeout";
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
      return {
        oldSmt: input.oldSmt,
        newSmt: input.newSmt,
        timeoutMs: input.timeoutMs ?? 5000,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as CheckImplicationOutput;
    },

    async run(input) {
      const binary = input.z3Binary ?? "z3";
      const timeoutMs = input.timeoutMs ?? 5000;

      // Probe A: P_new ∧ ¬P_old. unsat → P_new strengthens (or equals) P_old.
      const newImpliesOld = await z3CheckSat(
        binary,
        wrapImplicationProbe(input.newSmt, input.oldSmt),
        timeoutMs,
      );

      // Probe B: P_old ∧ ¬P_new. unsat → P_new weakens (or equals) P_old.
      const oldImpliesNew = await z3CheckSat(
        binary,
        wrapImplicationProbe(input.oldSmt, input.newSmt),
        timeoutMs,
      );

      let verdict: ImplicationVerdict;
      if (newImpliesOld === "unknown" || newImpliesOld === "timeout" ||
          oldImpliesNew === "unknown" || oldImpliesNew === "timeout") {
        verdict = "undecidable";
      } else if (newImpliesOld === "unsat" && oldImpliesNew === "unsat") {
        verdict = "equivalent";
      } else if (newImpliesOld === "unsat" && oldImpliesNew === "sat") {
        verdict = "strengthened";
      } else if (newImpliesOld === "sat" && oldImpliesNew === "unsat") {
        verdict = "weakened";
      } else {
        verdict = "incomparable";
      }

      return { verdict, newImpliesOld, oldImpliesNew };
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

async function z3CheckSat(
  binary: string,
  script: string,
  timeoutMs: number,
): Promise<"sat" | "unsat" | "unknown" | "timeout"> {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(binary, ["-in", "-T:" + Math.ceil(timeoutMs / 1000)], { stdio: ["pipe", "pipe", "pipe"] });
    } catch {
      resolve("unknown");
      return;
    }
    let stdout = "";
    if (child.stdout) child.stdout.on("data", (c) => (stdout += c.toString()));
    if (child.stderr) child.stderr.on("data", () => { /* discard */ });
    const timer = setTimeout(() => {
      try { child.kill("SIGKILL"); } catch { /* ignore */ }
      resolve("timeout");
    }, timeoutMs + 250);
    child.on("error", () => { clearTimeout(timer); resolve("unknown"); });
    child.on("close", () => {
      clearTimeout(timer);
      const lines = stdout.trim().split("\n").map((l) => l.trim());
      const last = lines[lines.length - 1] ?? "";
      if (last === "sat" || last === "unsat" || last === "unknown") resolve(last);
      else resolve("unknown");
    });
    if (child.stdin) {
      child.stdin.write(script);
      child.stdin.end();
    }
  });
}
