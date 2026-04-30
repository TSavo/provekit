/**
 * EnumerateInvariantPaths Stage — list dataflow paths to a stored
 * invariant's callsite via the substrate's path enumerator.
 *
 * Migration of `runInvariants("paths")` from src/cli.ts. Diagnostic
 * surface: useful for debugging the enumerator before / alongside the
 * Z3 path checker.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 */

import type { Stage } from "../types.js";

export const ENUMERATE_INVARIANT_PATHS_CAPABILITY = "enumerate-invariant-paths";

export interface EnumerateInvariantPathsInput {
  projectRoot: string;
  invariantId: string;
  maxPaths: number;
}

export interface PathStep {
  slot: string;
  nodeId: string;
}

export interface InvariantPath {
  steps: PathStep[];
}

export interface EnumerateInvariantPathsOutput {
  invariantId: string;
  filePath: string;
  startLine: number;
  paths: InvariantPath[];
}

export interface MakeEnumerateInvariantPathsStageDeps {
  /** Override producer identity. Default: "enumerateInvariantPaths@v1". */
  producerVersion?: string;
}

export function makeEnumerateInvariantPathsStage(
  deps: MakeEnumerateInvariantPathsStageDeps = {},
): Stage<EnumerateInvariantPathsInput, EnumerateInvariantPathsOutput> {
  const producedBy = deps.producerVersion ?? "enumerateInvariantPaths@v1";

  return {
    name: "enumerateInvariantPaths",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        invariantId: input.invariantId,
        maxPaths: input.maxPaths,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as EnumerateInvariantPathsOutput;
    },

    async run(input) {
      return runEnumerateInvariantPaths(input);
    },
  };
}

export async function runEnumerateInvariantPaths(
  input: EnumerateInvariantPathsInput,
): Promise<EnumerateInvariantPathsOutput> {
  const { readInvariants } = await import(
    "../../fix/runtime/invariantStore.js"
  );
  const { openSubstrateDb, resolveCallsiteNodeId } = await import(
    "../../fix/runtime/substrate.js"
  );
  const { pathsTo } = await import("../../fix/runtime/pathEnumerator.js");

  const inv = readInvariants(input.projectRoot, { includeRetired: true })
    .find((i) => i.id === input.invariantId);
  if (!inv) {
    throw new Error(`invariant ${input.invariantId} not found`);
  }

  const db = openSubstrateDb(input.projectRoot);
  if (!db) {
    throw new Error(
      ".provekit/provekit.db not found — run `provekit analyze` first",
    );
  }

  const nodeId = resolveCallsiteNodeId(
    db,
    inv.callsite.filePath,
    inv.callsite.startLine,
  );
  if (!nodeId) {
    throw new Error(
      `could not resolve callsite ${inv.callsite.filePath}:${inv.callsite.startLine} in substrate`,
    );
  }

  const paths = pathsTo(db, nodeId, { maxPaths: input.maxPaths });

  return {
    invariantId: inv.id,
    filePath: inv.callsite.filePath,
    startLine: inv.callsite.startLine,
    paths: paths.map((p) => ({
      steps: p.steps.map((s) => ({
        slot: s.slot,
        nodeId: s.nodeId,
      })),
    })),
  };
}
