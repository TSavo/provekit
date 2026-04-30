/**
 * locate-memento Stage — refute workflow's formula-recovery step.
 *
 * Given a propertyHash, look up the memento store and reconstruct the
 * IrFormula plus its identity tuple (bindingHash, propertyHash, source
 * memento CID). Subsequent stages translate the formula to SMT-LIB and
 * ask Z3 for a counterexample.
 *
 * Coupling note: v1 reads `formulate-via-lifter` mementos, whose
 * legacy-witness rawWitness is a JSON-serialized FormulateViaLifterStageOutput.
 * That stage's `formula` field is the IrFormula. If a different producer
 * minted the memento for the same propertyHash with an unrelated witness
 * shape, recovery fails with a clear error. A typed `formula` evidence
 * variant would lift that coupling — out of scope here; surfaced as a
 * spec gap in the commit.
 */

import { findMementoByPropertyHash } from "../../fix/runtime/mementoStore.js";
import type { Db } from "../../db/index.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { FormulateViaLifterStageOutput } from "./formulateViaLifter.js";
import type { Stage } from "../types.js";

export const LOCATE_MEMENTO_CAPABILITY = "locate-memento";

export interface LocateMementoStageInput {
  propertyHash: string;
}

export interface LocateMementoStageOutput {
  formula: IrFormula;
  propertyHash: string;
  bindingHash: string;
  /** CID of the source memento we recovered the formula from. */
  sourceCid: string;
  /** Producer identity of the source memento (e.g. "formulate-via-lifter@v1"). */
  sourceProducedBy: string;
}

export interface MakeLocateMementoStageDeps {
  db: Db;
  /** Override producer identity. Default: "locate-memento@v1". */
  producerVersion?: string;
}

export class LocateMementoError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "LocateMementoError";
  }
}

function tryExtractFormula(rawWitness: string): IrFormula | null {
  try {
    const parsed = JSON.parse(rawWitness) as Partial<FormulateViaLifterStageOutput>;
    if (parsed && typeof parsed === "object" && parsed.formula) {
      return parsed.formula as IrFormula;
    }
  } catch {
    // fall through
  }
  return null;
}

export function makeLocateMementoStage(
  deps: MakeLocateMementoStageDeps,
): Stage<LocateMementoStageInput, LocateMementoStageOutput> {
  const producedBy = deps.producerVersion ?? "locate-memento@v1";

  return {
    name: "locate-memento",
    producedBy,

    serializeInput(input) {
      return { propertyHash: input.propertyHash };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as LocateMementoStageOutput;
    },

    async run(input) {
      const candidates = findMementoByPropertyHash(deps.db, input.propertyHash);
      if (candidates.length === 0) {
        throw new LocateMementoError(
          `no memento found for propertyHash "${input.propertyHash}"`,
        );
      }

      // Walk candidates: first one whose witness contains a usable formula
      // wins. Prefer formulate-via-lifter producers but accept any memento
      // whose rawWitness round-trips to FormulateViaLifterStageOutput.
      const sorted = [...candidates].sort((a, b) => {
        const aIsFormulate = a.producedBy.startsWith("formulate-via-lifter");
        const bIsFormulate = b.producedBy.startsWith("formulate-via-lifter");
        if (aIsFormulate && !bIsFormulate) return -1;
        if (!aIsFormulate && bIsFormulate) return 1;
        return 0;
      });

      for (const memento of sorted) {
        if (memento.witness == null) continue;
        if (memento.cid == null) continue;
        const formula = tryExtractFormula(memento.witness);
        if (formula) {
          return {
            formula,
            propertyHash: input.propertyHash,
            bindingHash: memento.bindingHash,
            sourceCid: memento.cid,
            sourceProducedBy: memento.producedBy,
          };
        }
      }

      throw new LocateMementoError(
        `propertyHash "${input.propertyHash}" matched ${candidates.length} memento(s) but none carried an extractable IrFormula`,
      );
    },
  };
}
