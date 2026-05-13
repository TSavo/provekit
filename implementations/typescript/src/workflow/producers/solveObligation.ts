/**
 * solve-obligation: Stage 4 of the bridge enforcement workflow.
 *
 * Takes a per-callsite IR formula obligation plus the configured
 * Solver (composite of one or more SolverEntry per provekit.config.yaml)
 * and returns the verdict.
 *
 * Architectural commitment (per the project's IR substrate):
 *   - Stage takes IR formulas, not SMT-LIB strings
 *   - emitSmtLibProblem translates IR → SMT-LIB at the dispatcher edge
 *   - Each SolverEntry's `compiler` field selects the IR translator
 *     (today: "smt-lib"; future: "lean", "coq" with their own emitters)
 *   - Multiple entries → all run in parallel; verdict = consensus or
 *     "unknown" (per the existing checkImplication composition rule)
 *
 * The obligation we want to discharge is the precondition formula
 * instantiated at the call site. We probe by checking the negation:
 *   `(assert (not OBLIGATION)) (check-sat)`
 *   - unsat → no counter-example exists; obligation holds for ALL
 *     reachable values; verdict is "discharged"
 *   - sat → a counter-example exists; the obligation can be falsified;
 *     verdict is "unsatisfied" with the model as witness
 *   - unknown / timeout → the solver gave up; verdict is "undecidable"
 */

import type { Stage } from "../types.js";
import { invokeSolver, type Solver } from "./checkImplication.js";
import { emitSmtLibProblem } from "../../ir/smt/index.js";
import type { IrFormula } from "../../ir/formulas.js";

export const SOLVE_OBLIGATION_CAPABILITY = "solve-obligation";

export interface SolveObligationInput {
  obligation: IrFormula;
  solver: Solver;
}

export type ObligationVerdict =
  | "discharged"
  | "unsatisfied"
  | "undecidable"
  | "disagreement";

export interface SolveObligationOutput {
  verdict: ObligationVerdict;
  /** Per-entry probe verdicts (always populated; one entry → one element). */
  perEntry: Array<{ solverType: string; probe: "sat" | "unsat" | "unknown" | "timeout" }>;
  /** True iff all entries returned the same probe verdict. */
  allAgreed: boolean;
  /** SMT-LIB script that was fed to each entry (debug/audit). */
  script: string;
}

export interface MakeSolveObligationStageDeps {
  producerVersion?: string;
}

export function makeSolveObligationStage(
  deps: MakeSolveObligationStageDeps = {},
): Stage<SolveObligationInput, SolveObligationOutput> {
  const producedBy = deps.producerVersion ?? "solveObligation@v1";

  return {
    name: "solveObligation",
    producedBy,

    serializeInput(input) {
      // Cache key: obligation IR + sorted solver entries (so cache hits
      // across config reorderings).
      const entries = [...input.solver.entries]
        .sort((a, b) => a.type.localeCompare(b.type))
        .map((e) => ({
          type: e.type,
          binary: e.binary,
          compiler: e.compiler,
          flags: e.flags,
          timeoutMs: e.timeoutMs,
        }));
      return { obligation: input.obligation, solverEntries: entries };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as SolveObligationOutput;
    },

    async run(input) {
      // Reject non-smt-lib compilers up front: same posture as
      // checkImplication. Proof-assistant routes need their own emitters.
      for (const entry of input.solver.entries) {
        if (entry.compiler && entry.compiler !== "smt-lib") {
          throw new Error(
            `solveObligation: solver "${entry.type}" uses compiler "${entry.compiler}", ` +
              `but this Stage only supports "smt-lib". Proof-assistant obligations ` +
              `would need their own emitter and a separate stage.`,
          );
        }
      }

      // Use the existing emitSmtLibProblem: negates the assertion and
      // appends (check-sat). axioms is empty here: the obligation is
      // self-contained; if the property memento needed axioms (e.g.,
      // theory of strings), a future revision can carry them via the
      // property memento body.
      const script = emitSmtLibProblem({ axioms: [], assertion: input.obligation });

      const perEntry = await Promise.all(
        input.solver.entries.map(async (entry) => {
          const probe = await invokeSolver(entry, script);
          return { solverType: entry.type, probe };
        }),
      );

      const probes = perEntry.map((e) => e.probe);
      const allAgreed = probes.every((p) => p === probes[0]);
      let verdict: ObligationVerdict;
      if (!allAgreed) {
        verdict = "disagreement";
      } else {
        const consensus = probes[0]!;
        verdict =
          consensus === "unsat"
            ? "discharged"
            : consensus === "sat"
              ? "unsatisfied"
              : "undecidable";
      }
      return { verdict, perEntry, allAgreed, script };
    },
  };
}
