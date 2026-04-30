/**
 * plan-hook-operation Stage — decide what the manage-git-hook Action
 * should do.
 *
 * Pure: takes a desired operation (install / uninstall / status) and a
 * project root, returns a structured plan. The plan is content-
 * addressable; two runs with the same inputs produce identical plans
 * and identical mementos. The actual filesystem mutation is the
 * downstream Action's job.
 *
 * Why not skip straight to the Action: planning what to do is a CLAIM
 * about the desired end-state. Recording it as a Stage memento means a
 * future audit walk can see "the developer asked for hook=installed"
 * even if the Action's filesystem mutation drifted afterwards.
 */

import type { Stage } from "../types.js";

export const PLAN_HOOK_OPERATION_CAPABILITY = "plan-hook-operation";

export type HookOperation = "install" | "uninstall" | "status";

export interface PlanHookOperationStageInput {
  operation: HookOperation;
  /** Repository root the hook is being installed into. */
  projectRoot: string;
}

export interface PlanHookOperationStageOutput {
  operation: HookOperation;
  projectRoot: string;
}

export interface MakePlanHookOperationStageDeps {
  /** Override producer identity. Default: "plan-hook-operation@v1". */
  producerVersion?: string;
}

const VALID_OPERATIONS: ReadonlySet<HookOperation> = new Set([
  "install",
  "uninstall",
  "status",
]);

export function makePlanHookOperationStage(
  deps: MakePlanHookOperationStageDeps = {},
): Stage<PlanHookOperationStageInput, PlanHookOperationStageOutput> {
  const producedBy = deps.producerVersion ?? "plan-hook-operation@v1";

  return {
    name: "plan-hook-operation",
    producedBy,

    serializeInput(input) {
      return {
        operation: input.operation,
        projectRoot: input.projectRoot,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as PlanHookOperationStageOutput;
    },

    async run(input) {
      if (!VALID_OPERATIONS.has(input.operation)) {
        throw new Error(
          `plan-hook-operation: unknown operation "${input.operation}" (expected install|uninstall|status)`,
        );
      }
      if (typeof input.projectRoot !== "string" || input.projectRoot.length === 0) {
        throw new Error("plan-hook-operation requires a non-empty projectRoot");
      }
      return { operation: input.operation, projectRoot: input.projectRoot };
    },
  };
}
