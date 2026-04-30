/**
 * emit-lean Stage — prove-with-lean workflow's IR -> Lean 4 step.
 *
 * Wraps `emitLeanTheorem` from `src/ir/lean/`. Pure, deterministic, no I/O.
 * Output is a Lean 4 source string for the theorem statement (proof body
 * is `sorry`); the downstream `provideLeanProof` Action concatenates the
 * caller's proof file and runs `lean` on the combined input.
 *
 * Per scope discipline (protocol/specs/2026-04-29-the-semantic-envelope.md),
 * this stage MINTS Lean source. It does not invoke `lean` — that's the
 * Action's job.
 */

import { emitLeanTheorem, LeanUnsupportedError } from "../../ir/lean/index.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { Stage } from "../types.js";

export const EMIT_LEAN_CAPABILITY = "emit-lean";

export interface EmitLeanStageInput {
  /** The assertion to express as a theorem statement. */
  formula: IrFormula;
  /** Kit-supplied axioms folded into the statement as implications. */
  axioms?: IrFormula[];
  /** Optional explicit theorem name. Defaults to a content-addressed prop_<hash16>. */
  theoremName?: string;
}

export interface EmitLeanStageOutput {
  /** Full Lean source: axioms + `theorem <name> : <prop> := by sorry`. */
  source: string;
  /** The theorem identifier the source declares. */
  theoremName: string;
}

export interface MakeEmitLeanStageDeps {
  /** Override producer identity. Default: "emit-lean@v1". */
  producerVersion?: string;
}

export function makeEmitLeanStage(
  deps: MakeEmitLeanStageDeps = {},
): Stage<EmitLeanStageInput, EmitLeanStageOutput> {
  const producedBy = deps.producerVersion ?? "emit-lean@v1";

  return {
    name: "emit-lean",
    producedBy,

    serializeInput(input) {
      return {
        formula: input.formula,
        axioms: input.axioms ?? [],
        theoremName: input.theoremName ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as EmitLeanStageOutput;
    },

    async run(input) {
      try {
        const result = emitLeanTheorem({
          axioms: input.axioms ?? [],
          assertion: input.formula,
          ...(input.theoremName !== undefined ? { name: input.theoremName } : {}),
        });
        return { source: result.source, theoremName: result.theoremName };
      } catch (err) {
        if (err instanceof LeanUnsupportedError) {
          throw err;
        }
        throw err;
      }
    },
  };
}
