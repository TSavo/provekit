/**
 * emit-cvc5-smt-lib Stage — CVC5-flavored IR → SMT-LIB translation step.
 *
 * Wraps `emitCvc5SmtLibProblem` from `src/ir/cvc5/`. Pure, deterministic,
 * no I/O. Output is a single SMT-LIB string the downstream invoke-cvc5
 * stage feeds to cvc5 on stdin.
 *
 * Parallel to emitSmtLib.ts (Z3-flavored). Both Stages take the SAME
 * IR formula as input but mint distinct output (different cache slot,
 * different CID, different producer identity). Two leaves attached to
 * one IR root — that composition is the load-bearing demonstration of
 * the architectural claim that propertyHash CIDs are solver-agnostic.
 *
 * Spec: docs/specs/2026-04-29-universal-claim-envelope.md
 */

import { emitCvc5SmtLibProblem } from "../../ir/cvc5/index.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { Stage } from "../types.js";

export const EMIT_CVC5_SMT_LIB_CAPABILITY = "emit-cvc5-smt-lib";

export interface EmitCvc5SmtLibStageInput {
  /** The assertion to verify (or refute). */
  formula: IrFormula;
  /** Kit-supplied axioms the solver should assume. */
  axioms?: IrFormula[];
  /** SMT-LIB logic. Defaults to "ALL". */
  logic?: string;
}

export interface EmitCvc5SmtLibStageOutput {
  /** Full SMT-LIB script: options + set-logic + declarations + axioms + (assert (not assertion)) + (check-sat). */
  smtLib: string;
  logic: string;
}

export interface MakeEmitCvc5SmtLibStageDeps {
  /** Override producer identity. Default: "emit-cvc5-smt-lib@v1". */
  producerVersion?: string;
}

export function makeEmitCvc5SmtLibStage(
  deps: MakeEmitCvc5SmtLibStageDeps = {},
): Stage<EmitCvc5SmtLibStageInput, EmitCvc5SmtLibStageOutput> {
  const producedBy = deps.producerVersion ?? "emit-cvc5-smt-lib@v1";

  return {
    name: "emit-cvc5-smt-lib",
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
      return JSON.parse(witness) as EmitCvc5SmtLibStageOutput;
    },

    async run(input) {
      const logic = input.logic ?? "ALL";
      const smtLib = emitCvc5SmtLibProblem({
        axioms: input.axioms ?? [],
        assertion: input.formula,
        logic,
      });
      return { smtLib, logic };
    },
  };
}
