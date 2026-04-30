/**
 * locate-workflow Stage — second node of the meta-dispatcher.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 *
 * Pure over `(command, manifestPaths)`. Given a command name and a
 * map of `command-name → manifest path on disk`, reads the matching
 * YAML and returns the parsed `WorkflowManifest`. The Stage does NOT
 * walk a directory — directory enumeration happens once in cli.ts at
 * startup (the same place the cliBlocks map is built), and the
 * resulting paths are threaded into `$input.manifestPaths`.
 *
 * Why threaded paths and not a directory: a Stage's input is the
 * boundary of its determinism. Walking a directory inside `run()`
 * would make the cache incoherent across changes to that directory
 * that don't show up in the property hash.
 *
 * Cacheable: input is `(command, manifestPaths[command])` and the
 * full path map (so changing other workflows' paths still
 * invalidates — same shape we use for kit catalogs). Output is the
 * parsed manifest.
 */

import { readFileSync } from "fs";
import type { Stage } from "../types.js";
import { parseManifest, type WorkflowManifest } from "../manifest.js";

export const LOCATE_WORKFLOW_CAPABILITY = "locate-workflow";

export interface LocateWorkflowStageInput {
  /** Command name resolved by parse-argv. */
  command: string;
  /** Map keyed by command name → absolute path of that workflow's YAML. */
  manifestPaths: Record<string, string>;
}

export interface LocateWorkflowOutput {
  /** Command we located. */
  command: string;
  /** Parsed manifest, ready to feed `runManifest`. */
  workflow: WorkflowManifest;
}

export interface MakeLocateWorkflowStageDeps {
  producerVersion?: string;
  /**
   * Optional reader override. Defaults to `fs.readFileSync` UTF-8.
   * Tests pass a stub to avoid touching the filesystem.
   */
  readFile?: (path: string) => string;
}

export function makeLocateWorkflowStage(
  deps: MakeLocateWorkflowStageDeps = {},
): Stage<LocateWorkflowStageInput, LocateWorkflowOutput> {
  const producedBy = deps.producerVersion ?? "locate-workflow@v1";
  const readFile = deps.readFile ?? ((p: string) => readFileSync(p, "utf-8"));

  return {
    name: "locate-workflow",
    producedBy,

    serializeInput(input) {
      // Canonicalize: sort manifestPaths by key so two callers that
      // build the map in different orders hash the same.
      const sortedPaths: Record<string, string> = {};
      for (const k of Object.keys(input.manifestPaths).sort()) {
        sortedPaths[k] = input.manifestPaths[k]!;
      }
      return { command: input.command, manifestPaths: sortedPaths };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as LocateWorkflowOutput;
    },

    async run(input) {
      const path = input.manifestPaths[input.command];
      if (!path) {
        throw new Error(
          `locate-workflow: no manifest path registered for command "${input.command}"`,
        );
      }
      const yaml = readFile(path);
      const workflow = parseManifest(yaml);
      return { command: input.command, workflow };
    },
  };
}
