/**
 * compare-verdicts Stage — cross-solver workflow's verdict-agreement step.
 *
 * Pure Stage. Takes the verdicts from invoke-z3 and invoke-cvc5 (each
 * having run on its own solver-flavored SMT-LIB, but BOTH against the
 * same IR), maps each to the shared property-verdict alphabet
 * (holds / violated / undecidable), and reports whether they agree.
 *
 * Why this is its own Stage rather than collapsed into the mint-memento
 * Action: the agreement claim is content-addressable on the two solver
 * verdicts — identical inputs yield the identical comparison, so it
 * caches. Splitting it out lets a downstream Action mint a memento
 * referencing this Stage's CID, which is the operational test of the
 * "two leaves on one IR root" claim — both invoke-* mementos are leaves
 * on the IR CID, and this Stage produces a third leaf that depends on
 * BOTH.
 *
 * Disagreement is NOT a hard error. Z3 and CVC5 can legitimately disagree
 * on undecidable nonlinear arithmetic, on different timeout settings, or
 * when one solver returns "unknown" while the other resolves. The Stage
 * captures the disagreement as data so downstream consumers can audit it.
 *
 * Spec: docs/specs/2026-04-29-the-semantic-envelope.md §"propertyHash
 * CID is canonical identity; solver verdicts are downstream Stages
 * that compose against it."
 */

import type { Stage } from "../types.js";
import type { Z3Verdict } from "./invokeZ3.js";
import type { Cvc5Verdict } from "./invokeCvc5.js";

export const COMPARE_VERDICTS_CAPABILITY = "compare-verdicts";

/** Shared property-verdict alphabet across solvers. */
export type PropertyVerdict = "holds" | "violated" | "undecidable";

export interface CompareVerdictsStageInput {
  z3Verdict: Z3Verdict;
  cvc5Verdict: Cvc5Verdict;
}

export interface CompareVerdictsStageOutput {
  /** Whether the two solvers reached the same property verdict. */
  agree: boolean;
  /**
   * The property verdict both solvers reached, if they agree. Undefined
   * on disagreement. Disagreement consumers read z3PropertyVerdict /
   * cvc5PropertyVerdict to see which side said what.
   */
  agreedVerdict?: PropertyVerdict;
  z3Verdict: Z3Verdict;
  cvc5Verdict: Cvc5Verdict;
  z3PropertyVerdict: PropertyVerdict;
  cvc5PropertyVerdict: PropertyVerdict;
}

function mapZ3(z3: Z3Verdict): PropertyVerdict {
  switch (z3) {
    case "unsat":
      return "holds";
    case "sat":
      return "violated";
    case "unknown":
    case "timeout":
      return "undecidable";
  }
}

function mapCvc5(cvc5: Cvc5Verdict): PropertyVerdict {
  switch (cvc5) {
    case "unsat":
      return "holds";
    case "sat":
      return "violated";
    case "unknown":
    case "timeout":
      return "undecidable";
  }
}

export interface MakeCompareVerdictsStageDeps {
  /** Override producer identity. Default: "compare-verdicts@v1". */
  producerVersion?: string;
}

export function makeCompareVerdictsStage(
  deps: MakeCompareVerdictsStageDeps = {},
): Stage<CompareVerdictsStageInput, CompareVerdictsStageOutput> {
  const producedBy = deps.producerVersion ?? "compare-verdicts@v1";

  return {
    name: "compare-verdicts",
    producedBy,

    serializeInput(input) {
      return {
        z3Verdict: input.z3Verdict,
        cvc5Verdict: input.cvc5Verdict,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as CompareVerdictsStageOutput;
    },

    async run(input) {
      const z3PropertyVerdict = mapZ3(input.z3Verdict);
      const cvc5PropertyVerdict = mapCvc5(input.cvc5Verdict);
      const agree = z3PropertyVerdict === cvc5PropertyVerdict;
      const out: CompareVerdictsStageOutput = {
        agree,
        z3Verdict: input.z3Verdict,
        cvc5Verdict: input.cvc5Verdict,
        z3PropertyVerdict,
        cvc5PropertyVerdict,
      };
      if (agree) {
        out.agreedVerdict = z3PropertyVerdict;
      }
      return out;
    },
  };
}
