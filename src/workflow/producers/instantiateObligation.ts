/**
 * instantiate-obligation — Stage 3 of the bridge enforcement workflow.
 *
 * Takes a property memento's IrFormula (a forall over the parameter
 * sort) plus the call site's lifted argument term, and returns the
 * obligation IrFormula obtained by dropping the outermost forall and
 * substituting the bound variable with the argument term.
 *
 * Result: a per-callsite IR formula that the solver dispatcher must
 * decide. For `forall s. nonempty(s)` and arg `Const "abc"`, the
 * obligation is `nonempty("abc")`.
 *
 * Returns obligation:null when the formula isn't a forall (the
 * property memento doesn't fit the precondition shape) or when
 * substitution would capture (the lifted term has free variables that
 * collide with inner bindings).
 */

import type { Stage } from "../types.js";
import { instantiateOutermostForall, SubstituteError } from "../../ir/substitute.js";
import type { IrFormula, IrTerm } from "../../ir/formulas.js";

export const INSTANTIATE_OBLIGATION_CAPABILITY = "instantiate-obligation";

export interface InstantiateObligationInput {
  formula: IrFormula;
  argTerm: IrTerm;
}

export interface InstantiateObligationOutput {
  obligation: IrFormula | null;
  /** When obligation is null, a structured reason for the report. */
  failureReason:
    | "formula-not-forall"
    | "substitution-capture"
    | "unknown"
    | null;
  failureMessage?: string;
}

export interface MakeInstantiateObligationStageDeps {
  producerVersion?: string;
}

export function makeInstantiateObligationStage(
  deps: MakeInstantiateObligationStageDeps = {},
): Stage<InstantiateObligationInput, InstantiateObligationOutput> {
  const producedBy = deps.producerVersion ?? "instantiateObligation@v1";

  return {
    name: "instantiateObligation",
    producedBy,

    serializeInput(input) {
      // Cache key: formula + argTerm. Two identical (formula, argTerm)
      // pairs always produce the same obligation; deterministic.
      return { formula: input.formula, argTerm: input.argTerm };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as InstantiateObligationOutput;
    },

    async run(input) {
      try {
        const obligation = instantiateOutermostForall(input.formula, input.argTerm);
        return { obligation, failureReason: null };
      } catch (e) {
        if (e instanceof SubstituteError) {
          if (/^instantiateOutermostForall expected a forall/.test(e.message)) {
            return {
              obligation: null,
              failureReason: "formula-not-forall",
              failureMessage: e.message,
            };
          }
          if (/^capture/.test(e.message)) {
            return {
              obligation: null,
              failureReason: "substitution-capture",
              failureMessage: e.message,
            };
          }
        }
        return {
          obligation: null,
          failureReason: "unknown",
          failureMessage: (e as Error).message,
        };
      }
    },
  };
}
