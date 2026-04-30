/**
 * RenderProofChain stage — local render of a memento's proof DAG.
 *
 * Given a starting CID, walks the consumer's LOCAL memento store via
 * mementoStore.walk() and produces a human-readable text rendering of
 * the proof chain. Pure read; no fs writes; no network.
 *
 * Scope discipline (CRITICAL):
 *   protocol/specs/2026-04-29-correctness-is-a-hash.md §"What ProvekIt is"
 *
 *   The framework operates on its OWN local leaves and lists its OWN
 *   roots. It does NOT walk into deeper-layer codebases. If a CID
 *   referenced by inputCids is not present in the local store, this
 *   Stage records "(not in local store)" and STOPS — auditors traverse
 *   externally with their own tooling.
 *
 *   Naming: this is `renderProofChain`, not `walkProofChain`. "Walk"
 *   implies traversing across DAGs the framework doesn't own. "Render"
 *   is the local-render-of-locally-loaded-DAG operation.
 *
 * The Stage is cacheable (pure: same db state + same startCid → same
 * rendering). The cache binds on (workflow.cid, "renderProofChain")
 * and the propertyHash is computed from the startCid + maxDepth. If
 * the underlying DB changes, the same startCid may render differently
 * — but the propertyHash will stay the same. This is acceptable
 * because the consumer's local DB is part of the local trust posture;
 * cache invalidation across DB changes is the consumer's policy.
 */

import { walk } from "../../fix/runtime/mementoStore.js";
import type { Memento } from "../../fix/runtime/mementoStore.js";
import type { Db } from "../../db/index.js";
import type { Stage } from "../types.js";

export const RENDER_PROOF_CHAIN_CAPABILITY = "render-proof-chain";

export interface RenderProofChainStageInput {
  /** CID of the memento to render the chain from. */
  startCid: string;
  /** Maximum DAG depth to traverse. Defaults to 100 inside walk(). */
  maxDepth?: number;
}

export interface RenderedMemento {
  cid: string;
  depth: number;
  bindingHash: string;
  propertyHash: string;
  verdict: string;
  producedBy: string;
  /**
   * Input CIDs the memento referenced. CIDs whose memento is NOT
   * present locally appear here as well — the renderer flags them
   * separately in `unresolvedInputCids`.
   */
  inputCids: string[];
}

export interface RenderProofChainOutput {
  /** The CID the render started from. */
  startCid: string;
  /** Whether the start CID resolved to a local memento. */
  startResolved: boolean;
  /** Mementos visited, in BFS order from the start. */
  mementos: RenderedMemento[];
  /**
   * inputCids referenced by visited mementos that did NOT resolve to
   * a local memento. Auditors take this list to their own tooling
   * and walk externally.
   */
  unresolvedInputCids: string[];
  /** Plain-text rendering. Multi-line. */
  text: string;
}

export interface MakeRenderProofChainStageDeps {
  db: Db;
  /** Override producer identity. Default: "renderProofChain@v1". */
  producerVersion?: string;
}

export function makeRenderProofChainStage(
  deps: MakeRenderProofChainStageDeps,
): Stage<RenderProofChainStageInput, RenderProofChainOutput> {
  const producedBy = deps.producerVersion ?? "renderProofChain@v1";

  return {
    name: "renderProofChain",
    producedBy,

    serializeInput(input) {
      return {
        startCid: input.startCid,
        maxDepth: input.maxDepth ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as RenderProofChainOutput;
    },

    async run(input) {
      const visited = walk(deps.db, input.startCid, {
        maxDepth: input.maxDepth ?? 100,
      });
      // Build a depth map by re-walking BFS — walk() returns BFS order
      // but doesn't expose depth; we recompute a parent-depth map.
      const depths = computeDepths(visited, input.startCid);

      const presentCids = new Set(visited.map((m) => m.cid!).filter(Boolean));
      const unresolved = new Set<string>();
      for (const m of visited) {
        for (const ic of m.inputCids ?? []) {
          if (!presentCids.has(ic)) unresolved.add(ic);
        }
      }

      const startResolved = visited.length > 0 && visited[0].cid === input.startCid;
      const renderedMementos: RenderedMemento[] = visited.map((m) => ({
        cid: m.cid ?? "",
        depth: depths.get(m.cid ?? "") ?? 0,
        bindingHash: m.bindingHash,
        propertyHash: m.propertyHash,
        verdict: m.verdict,
        producedBy: m.producedBy,
        inputCids: m.inputCids ?? [],
      }));

      const text = renderText({
        startCid: input.startCid,
        startResolved,
        mementos: renderedMementos,
        unresolvedInputCids: [...unresolved].sort(),
      });

      return {
        startCid: input.startCid,
        startResolved,
        mementos: renderedMementos,
        unresolvedInputCids: [...unresolved].sort(),
        text,
      };
    },
  };
}

function computeDepths(visited: Memento[], startCid: string): Map<string, number> {
  // walk() returns BFS order from startCid. Reconstruct depths by
  // looking up each memento's parent depth in the partial map.
  const depths = new Map<string, number>();
  const byCid = new Map<string, Memento>();
  for (const m of visited) if (m.cid) byCid.set(m.cid, m);

  // Each memento's depth = min(parent.depth) + 1, where "parent" is a
  // visited memento that lists this cid in its inputCids. The start
  // is depth 0.
  if (byCid.has(startCid)) depths.set(startCid, 0);

  // BFS-relax: walk visited in order, propagating depths.
  let changed = true;
  while (changed) {
    changed = false;
    for (const m of visited) {
      const cid = m.cid;
      if (!cid || !depths.has(cid)) continue;
      const d = depths.get(cid)!;
      for (const ic of m.inputCids ?? []) {
        if (!byCid.has(ic)) continue;
        const childDepth = d + 1;
        if (!depths.has(ic) || depths.get(ic)! > childDepth) {
          depths.set(ic, childDepth);
          changed = true;
        }
      }
    }
  }
  return depths;
}

function renderText(args: {
  startCid: string;
  startResolved: boolean;
  mementos: RenderedMemento[];
  unresolvedInputCids: string[];
}): string {
  const lines: string[] = [];
  lines.push(`Proof chain from ${args.startCid}`);
  if (!args.startResolved) {
    lines.push("  (start CID not in local store)");
    return lines.join("\n");
  }
  for (const m of args.mementos) {
    const indent = "  ".repeat(m.depth + 1);
    lines.push(
      `${indent}${m.cid} verdict=${m.verdict} producedBy=${m.producedBy} property=${m.propertyHash}`,
    );
  }
  if (args.unresolvedInputCids.length > 0) {
    lines.push("");
    lines.push("Unresolved inputCids (not in local store):");
    for (const cid of args.unresolvedInputCids) {
      lines.push(`  ${cid}`);
    }
  }
  return lines.join("\n");
}
