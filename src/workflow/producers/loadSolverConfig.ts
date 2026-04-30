/**
 * loadSolverConfig Stage — read provekit.config.yaml and emit the
 * framework's Solver value.
 *
 * Workflows that need a solver invoke this Stage to resolve the
 * project's configured solver (one entry or many under agreement
 * semantics). The Solver value is then passed by the workflow YAML to
 * downstream Stages like synthesize-meaning-diff or check-implication.
 *
 * Pure given (projectRoot, on-disk provekit.config.yaml). Cache-friendly:
 * unchanged config means unchanged Solver value means downstream Stages
 * cache-hit.
 *
 * No LLM. Pure file IO + schema validation.
 */

import type { Stage } from "../types.js";
import type { Solver } from "./checkImplication.js";
import { loadProvekitConfig } from "../../config/provekitConfig.js";

export const LOAD_SOLVER_CONFIG_CAPABILITY = "load-solver-config";

export interface LoadSolverConfigInput {
  projectRoot: string;
}

export interface MakeLoadSolverConfigStageDeps {
  producerVersion?: string;
}

export function makeLoadSolverConfigStage(
  deps: MakeLoadSolverConfigStageDeps = {},
): Stage<LoadSolverConfigInput, Solver> {
  const producedBy = deps.producerVersion ?? "loadSolverConfig@v1";

  return {
    name: "loadSolverConfig",
    producedBy,

    serializeInput(input) {
      return { projectRoot: input.projectRoot };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as Solver;
    },

    async run(input) {
      const config = loadProvekitConfig(input.projectRoot);
      // The Solver wraps the configured entries. Single-entry case is
      // just an entries array of length one; the framework calls every
      // Solver uniformly.
      return {
        entries: config.providers.solver.map((e) => ({
          type: e.type,
          binary: e.binary ?? e.type,
          flags: e.flags,
          timeoutMs: e.timeoutMs,
        })),
      };
    },
  };
}
