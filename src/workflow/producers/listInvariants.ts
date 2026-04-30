/**
 * ListInvariants Stage — read the per-codebase invariant store.
 *
 * Reads `.provekit/invariants/*.json` via readInvariants(), returning
 * every active invariant (and retired ones when includeRetired is true).
 * Migration of `runInvariants("list")` from src/cli.ts.
 *
 * Cacheability: same compromise as other filesystem-walking Stages —
 * pure given the on-disk content. Cache key is (projectRoot,
 * includeRetired).
 *
 * Spec: docs/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 */

import type { Stage } from "../types.js";

export const LIST_INVARIANTS_CAPABILITY = "list-invariants";

export interface ListInvariantsInput {
  projectRoot: string;
  includeRetired: boolean;
}

export interface ListedInvariant {
  id: string;
  kind: string;
  filePath: string;
  startLine: number;
  originatingBug: string;
  retired: boolean;
}

export interface ListInvariantsOutput {
  invariants: ListedInvariant[];
  /** True iff the .provekit/invariants directory exists. */
  storeExists: boolean;
}

export interface MakeListInvariantsStageDeps {
  /** Override producer identity. Default: "listInvariants@v1". */
  producerVersion?: string;
}

export function makeListInvariantsStage(
  deps: MakeListInvariantsStageDeps = {},
): Stage<ListInvariantsInput, ListInvariantsOutput> {
  const producedBy = deps.producerVersion ?? "listInvariants@v1";

  return {
    name: "listInvariants",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        includeRetired: input.includeRetired,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ListInvariantsOutput;
    },

    async run(input) {
      return runListInvariants(input);
    },
  };
}

export async function runListInvariants(
  input: ListInvariantsInput,
): Promise<ListInvariantsOutput> {
  const { existsSync } = await import("fs");
  const { join } = await import("path");
  const { readInvariants } = await import(
    "../../fix/runtime/invariantStore.js"
  );
  // Compute the store path WITHOUT calling invariantStoreDir(): that
  // helper has the side effect of creating the directory, which would
  // make storeExists meaningless. The Stage is read-only by contract.
  const storeDir = join(input.projectRoot, ".provekit", "invariants");
  const storeExists = existsSync(storeDir);
  if (!storeExists) {
    return { invariants: [], storeExists: false };
  }
  const invariants = readInvariants(input.projectRoot, {
    includeRetired: input.includeRetired,
  });
  return {
    storeExists: true,
    invariants: invariants.map((inv) => ({
      id: inv.id,
      kind: inv.smt.kind,
      filePath: inv.callsite.filePath,
      startLine: inv.callsite.startLine,
      originatingBug: inv.originatingBug,
      retired: !!inv.retired,
    })),
  };
}
