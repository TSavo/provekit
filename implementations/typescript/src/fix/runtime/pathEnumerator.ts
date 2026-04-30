/**
 * Step 3 of the standing-invariant-runtime spec: path enumerator.
 *
 * Given a callsite node ID, walk the substrate's data_flow graph
 * backward and enumerate paths from data sources to the callsite. Each
 * path becomes a candidate the Z3 path checker (step 4) will evaluate
 * against the invariant's universal property.
 *
 * Algorithm: reverse BFS over data_flow edges (toNode ← fromNode), with
 * deduplication by visited-node-set and a cap on total paths returned.
 *
 * Termination:
 *   - Cycle: skipped (deduped node-set)
 *   - Source: a node with no incoming data_flow edges (literal,
 *             parameter, db read, external input)
 *   - Cap: K paths returned; more enumerable on demand
 *
 * v1 keeps the path representation minimal: an ordered list of nodeIds
 * with each step's slot label (the data_flow.slot column carries the
 * semantic role — "argument", "return", "assignment", etc.). The Z3
 * path checker reads slot to decide constraint propagation.
 */

import { eq } from "drizzle-orm";
import type { Db } from "../../db/index.js";
import { dataFlow, dataFlowTransitive } from "../../sast/schema/dataFlow.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface PathStep {
  /** Node ID at this step. */
  nodeId: string;
  /**
   * The slot the previous step's value flowed INTO this node through.
   * "source" for the first step (the path's data origin).
   * Subsequent steps carry the data_flow.slot value.
   */
  slot: string;
}

export interface Path {
  /**
   * Ordered list of steps from data source (first) to callsite (last).
   * The callsite IS the last entry; the source IS the first.
   */
  steps: PathStep[];
}

export interface EnumerateOptions {
  /**
   * Maximum number of paths to return. Default 50; raise via callsite-
   * specific config when an invariant explicitly opts in.
   */
  maxPaths?: number;
  /**
   * Maximum depth of any single path in steps. Default 32. Cycles get
   * caught earlier by the dedup set; this just bounds pathological
   * deeply-nested data flows.
   */
  maxDepth?: number;
}

// ---------------------------------------------------------------------------
// pathsTo
// ---------------------------------------------------------------------------

/**
 * Enumerate paths backward from a callsite to all data sources reachable
 * through the data_flow graph. Returns up to `maxPaths` distinct paths,
 * deduplicated by visited-node-set. Each path is an ordered list of
 * (nodeId, slot) steps from source (index 0) to callsite (last index).
 */
export function pathsTo(
  db: Db,
  callsiteNodeId: string,
  options: EnumerateOptions = {},
): Path[] {
  const maxPaths = options.maxPaths ?? 50;
  const maxDepth = options.maxDepth ?? 32;

  const out: Path[] = [];

  // Reverse BFS: each frontier entry is a partial path ending at the
  // current "frontier node," with steps in reverse-discovery order
  // (frontier first, callsite last). When we hit a node with no
  // incoming edges we reverse the steps and emit a complete path.
  type Frontier = {
    node: string;
    steps: PathStep[];
    /** Set of node ids visited in this branch — cycle detection. */
    visited: Set<string>;
  };

  const queue: Frontier[] = [
    {
      node: callsiteNodeId,
      steps: [{ nodeId: callsiteNodeId, slot: "callsite" }],
      visited: new Set([callsiteNodeId]),
    },
  ];

  while (queue.length > 0 && out.length < maxPaths) {
    const cur = queue.shift()!;

    if (cur.steps.length >= maxDepth) {
      // Treat depth-cap hit as a path termination — the path so far
      // has real informational value even truncated. Slot labels make
      // it obvious this didn't reach a true source.
      const reversed = [...cur.steps].reverse();
      reversed[0]!.slot = "depth-capped";
      out.push({ steps: reversed });
      continue;
    }

    const incoming = db
      .select({ fromNode: dataFlow.fromNode, slot: dataFlow.slot })
      .from(dataFlow)
      .where(eq(dataFlow.toNode, cur.node))
      .all();

    if (incoming.length === 0) {
      // Source: no incoming edges. Emit as a complete path (reversed
      // so the source is first and the callsite is last).
      const reversed = [...cur.steps].reverse();
      reversed[0]!.slot = "source";
      out.push({ steps: reversed });
      continue;
    }

    for (const edge of incoming) {
      if (cur.visited.has(edge.fromNode)) continue; // cycle
      const nextSteps: PathStep[] = [
        { nodeId: edge.fromNode, slot: edge.slot },
        ...cur.steps,
      ];
      const nextVisited = new Set(cur.visited);
      nextVisited.add(edge.fromNode);
      queue.push({
        node: edge.fromNode,
        steps: nextSteps,
        visited: nextVisited,
      });
    }
  }

  return out;
}

// ---------------------------------------------------------------------------
// canReach (transitive-closure shortcut)
// ---------------------------------------------------------------------------

/**
 * Fast yes/no: does any path exist from `fromNode` to `toNode`? Uses
 * the precomputed `data_flow_transitive` table, no enumeration. Useful
 * for adversarial scan ("are there any callers of forRevision in the
 * codebase that haven't been verified?") before paying the cost of
 * full path enumeration.
 */
export function canReach(db: Db, fromNode: string, toNode: string): boolean {
  const row = db
    .select({ fromNode: dataFlowTransitive.fromNode })
    .from(dataFlowTransitive)
    .where(eq(dataFlowTransitive.toNode, toNode))
    .all();
  return row.some((r) => r.fromNode === fromNode);
}

/**
 * All nodes that can reach the given target node (including the target
 * itself, via the transitive closure's reflexive entries when written).
 * The list of "everywhere this could feed from." Adversarial scan
 * iterates this list and runs path enumeration from each.
 */
export function reverseReachableNodes(db: Db, toNode: string): string[] {
  const rows = db
    .select({ fromNode: dataFlowTransitive.fromNode })
    .from(dataFlowTransitive)
    .where(eq(dataFlowTransitive.toNode, toNode))
    .all();
  return rows.map((r) => r.fromNode);
}
