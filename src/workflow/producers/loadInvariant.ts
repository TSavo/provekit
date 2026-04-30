/**
 * loadInvariant Stage — read a stored invariant by id.
 *
 * The invariant store at `.provekit/invariants/<id>.json` is the file-
 * system home of every minted invariant. This Stage is the workflow's
 * read entry point. Pure given (projectRoot, id): same id at same project
 * root yields the same StoredInvariant body.
 *
 * Used by the reevaluate-invariant workflow as its first stage; downstream
 * stages consume the StoredInvariant to find the current function body
 * and ask the LLM whether the invariant survives the edit.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import type { Stage } from "../types.js";
import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";

export const LOAD_INVARIANT_CAPABILITY = "load-invariant";

export interface LoadInvariantInput {
  /** Absolute project root. */
  projectRoot: string;
  /** sha256-prefix id of the invariant; matches the filename root. */
  id: string;
}

export interface MakeLoadInvariantStageDeps {
  producerVersion?: string;
}

export function makeLoadInvariantStage(
  deps: MakeLoadInvariantStageDeps = {},
): Stage<LoadInvariantInput, StoredInvariant> {
  const producedBy = deps.producerVersion ?? "loadInvariant@v1";

  return {
    name: "loadInvariant",
    producedBy,

    serializeInput(input) {
      return { projectRoot: input.projectRoot, id: input.id };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as StoredInvariant;
    },

    async run(input) {
      const path = join(input.projectRoot, ".provekit", "invariants", `${input.id}.json`);
      if (!existsSync(path)) {
        throw new Error(`loadInvariant: ${input.id} not found at ${path}`);
      }
      return JSON.parse(readFileSync(path, "utf-8")) as StoredInvariant;
    },
  };
}
