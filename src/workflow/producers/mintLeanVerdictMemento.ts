/**
 * mint-lean-verdict-memento Action — prove-with-lean's verdict-memento step.
 *
 * Parallel to mintVerdictMemento (the Z3 verdict-minter), but maps Lean's
 * verdict shape (valid / invalid / timeout / error) onto the same Verdict
 * enum (holds / violated / undecidable). Cross-paradigm composition: SMT
 * verdict mementos and Lean verdict mementos attach as separate leaves
 * to the same (bindingHash, propertyHash) pair.
 *
 * Verdict mapping:
 *   lean valid    -> property holds         (proof checked)
 *   lean invalid  -> property undecidable    (proof failed; that does NOT prove
 *                                            violation, only that the supplied
 *                                            proof did not check)
 *   lean timeout  -> property undecidable
 *   lean error    -> property undecidable
 *
 * Note the asymmetry with Z3: Z3 sat IS a counterexample (property
 * violated). Lean failure to check is NOT a counterexample — it just
 * means the user's proof didn't go through. Refutation requires a
 * different artifact (a Lean proof of negation), out of scope for v1.
 */

import { writeMemento } from "../../fix/runtime/mementoStore.js";
import type { Verdict } from "../../fix/runtime/mementoStore.js";
import type { Db } from "../../db/index.js";
import type { Action } from "../types.js";
import type { LeanVerdict } from "./provideLeanProof.js";

export const MINT_LEAN_VERDICT_MEMENTO_CAPABILITY = "mint-lean-verdict-memento";

export interface MintLeanVerdictMementoInput {
  /** Original binding (from locate-memento). */
  bindingHash: string;
  /** Original property (from locate-memento). */
  propertyHash: string;
  /** Lean's verdict on the proof. */
  leanVerdict: LeanVerdict;
  /** Combined .lean source (theorem + proof) sent to lean. */
  leanSource: string;
  /** Wall-clock duration of the lean run. */
  leanRunMs: number;
  /** Lean version best-effort. */
  leanVersion?: string;
  /** CIDs of upstream Stage mementos (locate-memento, emit-lean). */
  inputCids: string[];
  /** Optional producer override; defaults to the Action's identity. */
  producedBy?: string;
}

export interface MintLeanVerdictMementoResource {
  cid: string;
  verdict: Verdict;
}

export interface MakeMintLeanVerdictMementoActionDeps {
  db: Db;
  producerVersion?: string;
}

function mapVerdict(lean: LeanVerdict): Verdict {
  switch (lean) {
    case "valid":
      return "holds";
    case "invalid":
    case "timeout":
    case "error":
      return "undecidable";
  }
}

export function makeMintLeanVerdictMementoAction(
  deps: MakeMintLeanVerdictMementoActionDeps,
): Action<MintLeanVerdictMementoInput, MintLeanVerdictMementoResource> {
  const producedBy = deps.producerVersion ?? "mint-lean-verdict-memento@v1";

  return {
    name: "mint-lean-verdict-memento",
    producedBy,

    serializeInput(input) {
      return {
        bindingHash: input.bindingHash,
        propertyHash: input.propertyHash,
        leanVerdict: input.leanVerdict,
        producedBy: input.producedBy ?? producedBy,
        inputCids: [...input.inputCids].sort(),
      };
    },

    describeResource(resource) {
      return `lean verdict ${resource.verdict} memento ${resource.cid}`;
    },

    async run(input) {
      const verdict = mapVerdict(input.leanVerdict);

      const witness = JSON.stringify({
        leanVerdict: input.leanVerdict,
        leanRunMs: input.leanRunMs,
        leanVersion: input.leanVersion ?? null,
        leanSource: input.leanSource,
      });

      const row = writeMemento(deps.db, {
        bindingHash: input.bindingHash,
        propertyHash: input.propertyHash,
        verdict,
        producedBy: input.producedBy ?? producedBy,
        inputCids: [...input.inputCids].sort(),
        witness,
      });

      return { cid: row.cid!, verdict };
    },
  };
}
