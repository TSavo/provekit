/**
 * invoke-workflow Action — terminal node of the meta-dispatcher.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 *
 * Side-effecting: invokes a real workflow against a real DB / LLM /
 * filesystem. Audit-only memento records that the dispatch ran. The
 * resource is the located workflow's terminal output.
 *
 * Why an Action and not a Stage:
 *   The dispatcher's substrate is the per-command **registration map** —
 *   command name → factory function that builds (registry, actionRegistry)
 *   from a deps bundle. Factory functions are runtime closures; they
 *   cannot round-trip through JSON. A Stage requires
 *   serializeOutput/deserializeOutput to reconstruct its result on cache
 *   hit. An Action does not — it always runs. That's the right shape
 *   for the dispatch step.
 *
 *   The substages — parse-argv and locate-workflow — ARE pure Stages,
 *   so cli.ts → dispatch_workflow can still cache argv parsing and
 *   manifest parsing across invocations. Only the final `runManifest`
 *   call lives outside the cache (and that's where caching happens
 *   inside the invoked workflow itself, by stage).
 */

import { runManifest, type WorkflowManifest } from "../manifest.js";
import { WorkflowRunner } from "../runner.js";
import type { Action } from "../types.js";
import type {
  ActionRegistry,
  ProducerRegistry,
} from "../registry.js";
import type { Db } from "../../db/index.js";

export const INVOKE_WORKFLOW_CAPABILITY = "invoke-workflow";

/**
 * The shape every per-command factory must satisfy. Returns the
 * registry pair the runner needs. Deps shape is intentionally `unknown`
 * here — each factory destructures the fields it cares about; the
 * dispatcher just threads the same deps bundle to every factory.
 */
export type RegistryFactory = (deps: unknown) => {
  registry: ProducerRegistry;
  actionRegistry?: ActionRegistry;
};

export interface RegistryFactoryMap {
  [command: string]: RegistryFactory;
}

export interface InvokeWorkflowActionInput {
  /** The located, parsed manifest from locate-workflow. */
  workflow: WorkflowManifest;
  /** Per-command registry-assembly map. */
  factories: RegistryFactoryMap;
  /** Deps bundle threaded into the chosen factory. */
  deps: unknown;
  /**
   * The workflow's actual `$input` payload, built by cli.ts from the
   * parsed argv (per the chosen workflow's `cli:` block) plus any
   * runtime context (projectRoot, etc.) the workflow expects.
   */
  workflowInput: unknown;
}

export interface InvokeWorkflowResource {
  /** Workflow name we invoked, captured for the audit memento. */
  workflowName: string;
  /** Workflow CID we invoked. */
  workflowCid: string;
  /** Terminal Stage CID of the invoked run. */
  terminalCid: string;
  /** Whether the workflow-level cache hit. */
  cacheHit: boolean;
  /** The workflow's output, threaded back to the caller. */
  output: unknown;
}

export interface MakeInvokeWorkflowActionDeps {
  db: Db;
  producerVersion?: string;
}

export function makeInvokeWorkflowAction(
  deps: MakeInvokeWorkflowActionDeps,
): Action<InvokeWorkflowActionInput, InvokeWorkflowResource> {
  const producedBy = deps.producerVersion ?? "invoke-workflow@v1";

  return {
    name: "invoke-workflow",
    producedBy,

    serializeInput(input) {
      // Audit memento records what we invoked, NOT the runtime closures
      // (factories, deps) — those aren't serializable and they're not
      // identity-bearing for the dispatch event. The memento captures
      // the located workflow's identity + the input payload.
      return {
        workflowName: input.workflow.name,
        workflowCid: input.workflow.cid,
        workflowInput: input.workflowInput,
      };
    },

    describeResource(resource) {
      return JSON.stringify({
        workflowName: resource.workflowName,
        workflowCid: resource.workflowCid,
        terminalCid: resource.terminalCid,
        cacheHit: resource.cacheHit,
      });
    },

    async run(input) {
      const factory = input.factories[input.workflow.name];
      if (!factory) {
        throw new Error(
          `invoke-workflow: no registry factory registered for workflow "${input.workflow.name}"`,
        );
      }
      const { registry, actionRegistry } = factory(input.deps);

      const runner = new WorkflowRunner(
        deps.db,
        { name: input.workflow.name, cid: input.workflow.cid },
        registry,
      );

      const result = await runManifest(
        runner,
        registry,
        input.workflow,
        input.workflowInput,
        actionRegistry,
      );

      return {
        workflowName: input.workflow.name,
        workflowCid: input.workflow.cid,
        terminalCid: result.cid,
        cacheHit: result.cacheHit,
        output: result.output,
      };
    },
  };
}
