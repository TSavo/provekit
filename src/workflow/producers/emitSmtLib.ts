/**
 * emit-smt-lib Stage — refute workflow's IR → SMT-LIB translation step.
 *
 * Wraps `emitSmtLibProblem` from `src/ir/smt/`. Pure, deterministic, no
 * I/O. Output is a single SMT-LIB string the downstream invoke-z3 stage
 * feeds to z3 on stdin.
 *
 * Per scope discipline (protocol/specs/2026-04-29-correctness-is-a-hash.md
 * §"What ProvekIt is"), this stage MINTS SMT-LIB. It does not invoke a
 * solver — that's invoke-z3's job.
 */

import { emitSmtLibProblem } from "../../ir/smt/index.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { Stage } from "../types.js";

export const EMIT_SMT_LIB_CAPABILITY = "emit-smt-lib";

export interface EmitSmtLibStageInput {
  /** The assertion to verify (or refute). */
  formula: IrFormula;
  /** Kit-supplied axioms the solver should assume. */
  axioms?: IrFormula[];
  /** SMT-LIB logic. Defaults to "ALL". */
  logic?: string;
}

export interface EmitSmtLibStageOutput {
  /** Full SMT-LIB script: preamble + declarations + axioms + (assert (not assertion)) + (check-sat). */
  smtLib: string;
  logic: string;
}

export interface MakeEmitSmtLibStageDeps {
  /** Override producer identity. Default: "emit-smt-lib@v1". */
  producerVersion?: string;
}

export function makeEmitSmtLibStage(
  deps: MakeEmitSmtLibStageDeps = {},
): Stage<EmitSmtLibStageInput, EmitSmtLibStageOutput> {
  const producedBy = deps.producerVersion ?? "emit-smt-lib@v1";

  return {
    name: "emit-smt-lib",
    producedBy,

    serializeInput(input) {
      return {
        formula: input.formula,
        axioms: input.axioms ?? [],
        logic: input.logic ?? "ALL",
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as EmitSmtLibStageOutput;
    },

    async run(input) {
      const logic = input.logic ?? "ALL";
      const smtLib = emitSmtLibProblem({
        axioms: input.axioms ?? [],
        assertion: input.formula,
        logic,
      });
      return { smtLib, logic };
    },
  };
}
