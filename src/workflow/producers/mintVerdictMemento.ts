/**
 * mint-verdict-memento Action — refute workflow's verdict-side-effect step.
 *
 * Why an Action: the side effect is writing a memento at a DIFFERENT
 * (bindingHash, propertyHash) than the workflow's auto-derived Stage
 * key. The Stage runner writes mementos at (workflow.cid, stage.name)
 * keys describing the workflow position; this Action writes a memento
 * at the ORIGINAL (bindingHash, propertyHash) the locate-memento Stage
 * recovered, materializing the property's actual verdict. That
 * cross-key write is the genuine side effect that doesn't fit Stage
 * caching semantics.
 *
 * Two mementos per refute run:
 *   - Stage-mementos (locate-memento, emit-smt-lib, invoke-z3) — Stage
 *     position bindings; verdict "holds" meaning "the stage ran."
 *   - Property memento (this Action's writeMemento call) — at the
 *     original (bindingHash, propertyHash); verdict reflects the
 *     refute outcome (holds / violated / undecidable).
 *   - Audit memento (the Action's auto-emitted invocation memento) —
 *     verdict "holds" meaning "the action ran successfully."
 *
 * Verdict mapping (refute semantics — Z3 ran on `(assert (not P))`):
 *   z3 unsat   → property holds      + Z3UnsatEvidence
 *   z3 sat     → property violated   + Z3ModelEvidence (model = counterexample)
 *   z3 unknown → property undecidable + LegacyWitnessEvidence (no Z3UndecidableEvidence variant yet)
 *   z3 timeout → property undecidable + LegacyWitnessEvidence
 *
 * The Action's run() returns the freshly-minted property memento's CID
 * as its resource — useful for downstream callers that want to walk
 * the verdict back through the proof DAG.
 */

import { writeMemento } from "../../fix/runtime/mementoStore.js";
import type { Verdict } from "../../fix/runtime/mementoStore.js";
import type { Db } from "../../db/index.js";
import type { Action } from "../types.js";
import type { Z3Verdict } from "./invokeZ3.js";

export const MINT_VERDICT_MEMENTO_CAPABILITY = "mint-verdict-memento";

export interface MintVerdictMementoActionInput {
  /**
   * Original binding the verdict applies to (from locate-memento).
   * Note: in v1 this is the workflow-position bindingHash from the
   * source formulate-via-lifter memento, not a source-code shape
   * binding — Stage mementos use `hash({workflow.cid, stage.name})`
   * for their binding. Cross-validation by propertyHash still works;
   * source-binding semantics are a follow-up.
   */
  bindingHash: string;
  /** Original property the verdict resolves (from locate-memento). */
  propertyHash: string;
  /** Z3's verdict on the SMT-LIB (sat / unsat / unknown / timeout). */
  z3Verdict: Z3Verdict;
  /** SMT-LIB script that was sent to z3. */
  smtLib: string;
  /** Wall-clock duration of the z3 run. */
  z3RunMs: number;
  /**
   * Counterexample model (present only on sat). Keys are SMT-LIB names;
   * values are the parsed Z3Value objects.
   */
  counterexample?: Record<string, unknown>;
  /**
   * CIDs of the upstream Stage mementos this verdict composes against.
   * Typically [locateMementoCid, emitSmtLibCid, invokeZ3Cid].
   */
  inputCids: string[];
  /**
   * Producer identity to record on the verdict memento. Optional — the
   * Action falls back to its own producedBy if the workflow caller did
   * not thread one through.
   */
  producedBy?: string;
}

export interface MintVerdictMementoResource {
  /** CID of the freshly-minted property memento. */
  cid: string;
  /** The property verdict written. */
  verdict: Verdict;
}

export interface MakeMintVerdictMementoActionDeps {
  db: Db;
  /** Override producer identity for the audit memento. Default: "mint-verdict-memento@v1". */
  producerVersion?: string;
}

function mapVerdict(z3: Z3Verdict): Verdict {
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

function buildEvidenceHint(
  z3: Z3Verdict,
  smtLib: string,
  z3RunMs: number,
  counterexample?: Record<string, unknown>,
):
  | {
      kind: "z3-unsat";
      body: { smtLibInput: string; z3Verdict: "unsat"; z3RunMs: number };
    }
  | {
      kind: "z3-model";
      body: {
        smtLibInput: string;
        z3Verdict: "sat";
        model: string;
        counterexample: Record<string, unknown>;
        z3RunMs: number;
      };
    }
  | undefined {
  if (z3 === "unsat") {
    return {
      kind: "z3-unsat",
      body: { smtLibInput: smtLib, z3Verdict: "unsat", z3RunMs },
    };
  }
  if (z3 === "sat") {
    const model = counterexample ?? {};
    return {
      kind: "z3-model",
      body: {
        smtLibInput: smtLib,
        z3Verdict: "sat",
        model: JSON.stringify(model),
        counterexample: model,
        z3RunMs,
      },
    };
  }
  // undecidable / timeout — fall back to legacy-witness; no typed
  // Z3UndecidableEvidence variant exists yet (spec gap).
  return undefined;
}

export function makeMintVerdictMementoAction(
  deps: MakeMintVerdictMementoActionDeps,
): Action<MintVerdictMementoActionInput, MintVerdictMementoResource> {
  const producedBy = deps.producerVersion ?? "mint-verdict-memento@v1";

  return {
    name: "mint-verdict-memento",
    producedBy,

    serializeInput(input) {
      return {
        bindingHash: input.bindingHash,
        propertyHash: input.propertyHash,
        z3Verdict: input.z3Verdict,
        producedBy: input.producedBy ?? producedBy,
        inputCids: [...input.inputCids].sort(),
      };
    },

    describeResource(resource) {
      return `verdict ${resource.verdict} memento ${resource.cid}`;
    },

    async run(input) {
      const verdict = mapVerdict(input.z3Verdict);
      const evidenceHint = buildEvidenceHint(
        input.z3Verdict,
        input.smtLib,
        input.z3RunMs,
        input.counterexample,
      );

      const witnessFallback = JSON.stringify({
        z3Verdict: input.z3Verdict,
        z3RunMs: input.z3RunMs,
        smtLibInput: input.smtLib,
      });

      const row = writeMemento(deps.db, {
        bindingHash: input.bindingHash,
        propertyHash: input.propertyHash,
        verdict,
        // Verdict-memento producedBy threads through from the caller
        // (typically the z3 version string) so cross-validation can
        // distinguish refute runs across z3 versions. Falls back to
        // the Action's own identity when unset.
        producedBy: input.producedBy ?? producedBy,
        inputCids: [...input.inputCids].sort(),
        ...(evidenceHint
          ? { evidenceHint }
          : { witness: witnessFallback }),
      });

      return { cid: row.cid!, verdict };
    },
  };
}
