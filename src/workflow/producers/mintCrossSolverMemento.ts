/**
 * mint-cross-solver-memento Action. The prove-cross-solver workflow's
 * verdict-side-effect step.
 *
 * Why an Action: same shape as mint-verdict-memento. The side effect is
 * writing a memento at the ORIGINAL (bindingHash, propertyHash) the
 * locate-memento Stage recovered, NOT at the workflow-position key the
 * Stage runner would auto-derive. The cross-key write is what doesn't
 * fit Stage caching semantics.
 *
 * The minted memento records both solver verdicts and whether they
 * agreed. Disagreement is captured as data, not raised as an error;
 * Z3 and CVC5 disagreeing on undecidable nonlinear arithmetic is real.
 *
 * `inputCids` carries the source IR memento CID (and optionally the
 * intermediate Stage CIDs) so the verdict provenance walks back to the
 * IR. This is the operational test of the architectural claim:
 * propertyHash CID is canonical identity, both solver mementos AND this
 * cross-solver memento attach as leaves to it.
 *
 * Verdict mapping:
 *   agree && agreedVerdict      -> that verdict (holds / violated / undecidable)
 *   disagree                    -> "undecidable". The cross-solver claim
 *                                  is unresolved, even if individual solvers
 *                                  returned definite verdicts. Downstream
 *                                  consumers can drill into the witness
 *                                  to see who said what.
 */

import { writeMemento } from "../../fix/runtime/mementoStore.js";
import type { Verdict } from "../../fix/runtime/mementoStore.js";
import type { Db } from "../../db/index.js";
import type { Action } from "../types.js";
import type { Z3Verdict } from "./invokeZ3.js";
import type { Cvc5Verdict } from "./invokeCvc5.js";
import type { PropertyVerdict } from "./compareVerdicts.js";

export const MINT_CROSS_SOLVER_MEMENTO_CAPABILITY = "mint-cross-solver-memento";

export interface MintCrossSolverMementoActionInput {
  bindingHash: string;
  propertyHash: string;
  /** Whether the two solvers reached the same property verdict. */
  agree: boolean;
  /** The agreed property verdict, present iff `agree` is true. */
  agreedVerdict?: PropertyVerdict;
  z3Verdict: Z3Verdict;
  cvc5Verdict: Cvc5Verdict;
  z3PropertyVerdict: PropertyVerdict;
  cvc5PropertyVerdict: PropertyVerdict;
  /** SMT-LIB script sent to z3. Recorded for audit. */
  z3SmtLib: string;
  /** SMT-LIB script sent to cvc5. Recorded for audit. */
  cvc5SmtLib: string;
  /** Solver runtime metrics. */
  z3RunMs: number;
  cvc5RunMs: number;
  /**
   * CIDs to thread as memento inputs. Typically [sourceCid] (the source
   * IR memento) plus optionally the intermediate Stage CIDs. The IR
   * sourceCid is the architectural claim under test.
   */
  inputCids: string[];
  /** Producer identity to record on the memento. */
  producedBy?: string;
}

export interface MintCrossSolverMementoResource {
  cid: string;
  verdict: Verdict;
  agreement: boolean;
}

export interface MakeMintCrossSolverMementoActionDeps {
  db: Db;
  /** Override producer identity. Default: "mint-cross-solver-memento@v1". */
  producerVersion?: string;
}

function deriveVerdict(input: MintCrossSolverMementoActionInput): Verdict {
  if (input.agree && input.agreedVerdict) {
    return input.agreedVerdict;
  }
  return "undecidable";
}

export function makeMintCrossSolverMementoAction(
  deps: MakeMintCrossSolverMementoActionDeps,
): Action<MintCrossSolverMementoActionInput, MintCrossSolverMementoResource> {
  const producedBy = deps.producerVersion ?? "mint-cross-solver-memento@v1";

  return {
    name: "mint-cross-solver-memento",
    producedBy,

    serializeInput(input) {
      return {
        bindingHash: input.bindingHash,
        propertyHash: input.propertyHash,
        agree: input.agree,
        agreedVerdict: input.agreedVerdict ?? null,
        z3Verdict: input.z3Verdict,
        cvc5Verdict: input.cvc5Verdict,
        producedBy: input.producedBy ?? producedBy,
        inputCids: [...input.inputCids].sort(),
      };
    },

    describeResource(resource) {
      const tag = resource.agreement ? "agree" : "disagree";
      return `cross-solver verdict ${resource.verdict} (${tag}) memento ${resource.cid}`;
    },

    async run(input) {
      const verdict = deriveVerdict(input);
      const witnessPayload = {
        agree: input.agree,
        agreedVerdict: input.agreedVerdict ?? null,
        z3Verdict: input.z3Verdict,
        cvc5Verdict: input.cvc5Verdict,
        z3PropertyVerdict: input.z3PropertyVerdict,
        cvc5PropertyVerdict: input.cvc5PropertyVerdict,
        z3RunMs: input.z3RunMs,
        cvc5RunMs: input.cvc5RunMs,
        z3SmtLib: input.z3SmtLib,
        cvc5SmtLib: input.cvc5SmtLib,
      };

      const row = writeMemento(deps.db, {
        bindingHash: input.bindingHash,
        propertyHash: input.propertyHash,
        verdict,
        producedBy: input.producedBy ?? producedBy,
        inputCids: [...input.inputCids].sort(),
        witness: JSON.stringify(witnessPayload),
      });

      return {
        cid: row.cid!,
        verdict,
        agreement: input.agree,
      };
    },
  };
}
