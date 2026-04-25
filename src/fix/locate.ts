/**
 * B2: Locate — resolve a BugSignal's codeReferences to a structural BugLocus via SAST queries.
 *
 * Pure SAST query layer. No LLM calls, no new schema tables, no registries.
 */

import { eq, and, inArray } from "drizzle-orm";
import type { Db } from "../db/index.js";
import {
  files,
  nodes,
  nodeChildren,
  dataFlow,
  dominance,
  postDominance,
  nodeCalls,
  nodeBinding,
} from "../sast/schema/index.js";
import type { BugSignal, BugLocus, CodeReference } from "./types.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const FUNCTION_KINDS = new Set([
  "FunctionDeclaration",
  "ArrowFunction",
  "FunctionExpression",
  "MethodDeclaration",
  "GetAccessor",
  "SetAccessor",
  "Constructor",
]);

// Node kinds that are pure leaves / atomic tokens — deprioritized when picking
// the primary from a same-line cluster (so BinaryExpression wins over Identifier).
const LEAF_KINDS = new Set([
  "Identifier",
  "NumericLiteral",
  "StringLiteral",
  "NoSubstitutionTemplateLiteral",
  "TrueKeyword",
  "FalseKeyword",
  "NullKeyword",
  "UndefinedKeyword",
  "ThisKeyword",
]);

// Max related-function entries (callers + callees) to avoid O(N) blowup.
const MAX_RELATED = 50;

// ---------------------------------------------------------------------------
// Step 1: File resolution
// ---------------------------------------------------------------------------

/**
 * Resolve a ref.file string to a DB file row.
 * Accepts exact path match OR loose suffix match (e.g. "foo.ts" vs "/abs/path/foo.ts").
 * When multiple suffix matches, pick longest (most-specific).
 */
function resolveFile(
  db: Db,
  refFile: string,
): { id: number; path: string; rootNodeId: string } | null {
  // Try exact match first
  const exact = db
    .select({ id: files.id, path: files.path, rootNodeId: files.rootNodeId })
    .from(files)
    .where(eq(files.path, refFile))
    .get();
  if (exact) return exact;

  // Suffix scan: load all files and find those whose path ends with refFile
  // (preceded by a path separator so "foo.ts" doesn't match "barfoo.ts").
  const allFiles = db
    .select({ id: files.id, path: files.path, rootNodeId: files.rootNodeId })
    .from(files)
    .all();

  const suffix = refFile.startsWith("/") ? refFile : `/${refFile}`;
  const matches = allFiles.filter(
    (f) => f.path === refFile || f.path.endsWith(suffix),
  );

  if (matches.length === 0) return null;
  if (matches.length === 1) return matches[0]!;

  // Multiple matches: pick longest path (most-specific match)
  return matches.reduce((best, f) => (f.path.length > best.path.length ? f : best));
}

// ---------------------------------------------------------------------------
// Step 2: Candidate resolution
// ---------------------------------------------------------------------------

type MatchKind = "line" | "function" | "file";

interface Candidate {
  nodeId: string;
  matchKind: MatchKind;
  span: number; // sourceEnd - sourceStart (smaller = more specific)
  isLeaf: boolean;
  sourceLine: number;
  ref: CodeReference;
  fileRow: { id: number; path: string; rootNodeId: string };
}

function resolveCandidates(db: Db, signal: BugSignal): Candidate[] {
  const all: Candidate[] = [];

  for (const ref of signal.codeReferences) {
    const fileRow = resolveFile(db, ref.file);
    if (!fileRow) continue;

    if (ref.line !== undefined) {
      // Line-level match: all nodes starting on that line
      const lineNodes = db
        .select({
          id: nodes.id,
          kind: nodes.kind,
          sourceStart: nodes.sourceStart,
          sourceEnd: nodes.sourceEnd,
          sourceLine: nodes.sourceLine,
        })
        .from(nodes)
        .where(and(eq(nodes.fileId, fileRow.id), eq(nodes.sourceLine, ref.line)))
        .all();

      for (const n of lineNodes) {
        all.push({
          nodeId: n.id,
          matchKind: "line",
          span: n.sourceEnd - n.sourceStart,
          isLeaf: LEAF_KINDS.has(n.kind),
          sourceLine: n.sourceLine,
          ref,
          fileRow,
        });
      }
    } else if (ref.function) {
      // Function-name match: find FunctionDeclaration / etc. whose binding name matches
      const fnBindings = db
        .select({ nodeId: nodeBinding.nodeId })
        .from(nodeBinding)
        .innerJoin(nodes, eq(nodeBinding.nodeId, nodes.id))
        .where(
          and(
            eq(nodes.fileId, fileRow.id),
            eq(nodeBinding.name, ref.function),
          ),
        )
        .all();

      for (const b of fnBindings) {
        const n = db
          .select({
            id: nodes.id,
            kind: nodes.kind,
            sourceStart: nodes.sourceStart,
            sourceEnd: nodes.sourceEnd,
            sourceLine: nodes.sourceLine,
          })
          .from(nodes)
          .where(eq(nodes.id, b.nodeId))
          .get();
        if (!n) continue;
        all.push({
          nodeId: n.id,
          matchKind: "function",
          span: n.sourceEnd - n.sourceStart,
          isLeaf: LEAF_KINDS.has(n.kind),
          sourceLine: n.sourceLine,
          ref,
          fileRow,
        });
      }

      // Fallback: file root
      if (fnBindings.length === 0) {
        const rootNode = db
          .select({
            id: nodes.id,
            kind: nodes.kind,
            sourceStart: nodes.sourceStart,
            sourceEnd: nodes.sourceEnd,
            sourceLine: nodes.sourceLine,
          })
          .from(nodes)
          .where(eq(nodes.id, fileRow.rootNodeId))
          .get();
        if (rootNode) {
          all.push({
            nodeId: rootNode.id,
            matchKind: "file",
            span: rootNode.sourceEnd - rootNode.sourceStart,
            isLeaf: false,
            sourceLine: rootNode.sourceLine,
            ref,
            fileRow,
          });
        }
      }
    } else {
      // File-only match: use root node
      const rootNode = db
        .select({
          id: nodes.id,
          kind: nodes.kind,
          sourceStart: nodes.sourceStart,
          sourceEnd: nodes.sourceEnd,
          sourceLine: nodes.sourceLine,
        })
        .from(nodes)
        .where(eq(nodes.id, fileRow.rootNodeId))
        .get();
      if (rootNode) {
        all.push({
          nodeId: rootNode.id,
          matchKind: "file",
          span: rootNode.sourceEnd - rootNode.sourceStart,
          isLeaf: false,
          sourceLine: rootNode.sourceLine,
          ref,
          fileRow,
        });
      }
    }
  }

  return all;
}

// ---------------------------------------------------------------------------
// Step 3: Pick the primary node
// ---------------------------------------------------------------------------

function matchScore(c: Candidate): [number, number, number] {
  // [precision tier (higher = better), non-leaf bonus, negative span (larger span = worse)]
  const tier = c.matchKind === "line" ? 2 : c.matchKind === "function" ? 1 : 0;
  const leafPenalty = c.isLeaf ? 0 : 1; // prefer non-leaf for same-line candidates
  return [tier, leafPenalty, -c.span];
}

function pickPrimary(candidates: Candidate[]): Candidate | null {
  if (candidates.length === 0) return null;
  return candidates.reduce((best, c) => {
    const bs = matchScore(best);
    const cs = matchScore(c);
    for (let i = 0; i < bs.length; i++) {
      if (cs[i]! > bs[i]!) return c;
      if (cs[i]! < bs[i]!) return best;
    }
    return best;
  });
}

// ---------------------------------------------------------------------------
// Step 4: Walk parent chain to find containing function
// ---------------------------------------------------------------------------

function findContainingFunction(db: Db, nodeId: string, fileRootId: string): string {
  let currentId: string = nodeId;

  // Walk up via node_children (child → parent)
  // nc_by_child_id index makes this efficient.
  for (let depth = 0; depth < 200; depth++) {
    const parentEdge = db
      .select({ parentId: nodeChildren.parentId })
      .from(nodeChildren)
      .where(eq(nodeChildren.childId, currentId))
      .get();

    if (!parentEdge) break;

    const parentRow = db
      .select({ id: nodes.id, kind: nodes.kind })
      .from(nodes)
      .where(eq(nodes.id, parentEdge.parentId))
      .get();

    if (!parentRow) break;

    if (FUNCTION_KINDS.has(parentRow.kind)) {
      return parentRow.id;
    }

    currentId = parentRow.id;
  }

  return fileRootId;
}

// ---------------------------------------------------------------------------
// Step 5: Related functions (intra-file, capped)
// ---------------------------------------------------------------------------

function collectSubtreeIds(db: Db, rootId: string): Set<string> {
  const visited = new Set<string>();
  const stack = [rootId];
  while (stack.length > 0) {
    const id = stack.pop()!;
    if (visited.has(id)) continue;
    visited.add(id);
    const children = db
      .select({ childId: nodeChildren.childId })
      .from(nodeChildren)
      .where(eq(nodeChildren.parentId, id))
      .all();
    for (const c of children) stack.push(c.childId);
  }
  return visited;
}

function findRelatedFunctions(
  db: Db,
  containingFunctionId: string,
  fileId: number,
): string[] {
  const related = new Set<string>();

  // Get the function's binding name
  const bindingRow = db
    .select({ name: nodeBinding.name })
    .from(nodeBinding)
    .where(eq(nodeBinding.nodeId, containingFunctionId))
    .get();

  // Callers: node_calls rows in same file whose callee_node matches OR callee_name matches
  const callerQuery = db
    .select({ nodeId: nodeCalls.nodeId })
    .from(nodeCalls)
    .innerJoin(nodes, eq(nodeCalls.nodeId, nodes.id))
    .where(eq(nodes.fileId, fileId));

  const callerRows = callerQuery.all();

  for (const row of callerRows) {
    if (related.size >= MAX_RELATED) break;
    // Check if it calls our function
    const callRow = db
      .select({ calleeNode: nodeCalls.calleeNode, calleeName: nodeCalls.calleeName })
      .from(nodeCalls)
      .where(eq(nodeCalls.nodeId, row.nodeId))
      .get();
    if (!callRow) continue;
    const matchesNode = callRow.calleeNode === containingFunctionId;
    const matchesName =
      bindingRow && callRow.calleeName === bindingRow.name;
    if (matchesNode || matchesName) {
      // Walk up to find the enclosing function for this call site
      const enclosing = findContainingFunction(db, row.nodeId, containingFunctionId);
      if (enclosing !== containingFunctionId) {
        related.add(enclosing);
      }
    }
  }

  // Callees: collect subtree of containingFunction, find all call nodes, gather callee_node values
  const subtreeIds = collectSubtreeIds(db, containingFunctionId);
  const subtreeArr = [...subtreeIds];

  // Query in batches to stay under SQLite's expression limit
  const BATCH = 500;
  for (let i = 0; i < subtreeArr.length && related.size < MAX_RELATED; i += BATCH) {
    const batch = subtreeArr.slice(i, i + BATCH);
    const callRows = db
      .select({ calleeNode: nodeCalls.calleeNode })
      .from(nodeCalls)
      .where(inArray(nodeCalls.nodeId, batch))
      .all();
    for (const c of callRows) {
      if (related.size >= MAX_RELATED) break;
      if (c.calleeNode && c.calleeNode !== containingFunctionId) {
        related.add(c.calleeNode);
      }
    }
  }

  return [...related];
}

// ---------------------------------------------------------------------------
// Steps 6–7: Data flow + dominance queries
// ---------------------------------------------------------------------------

function queryDataFlowAncestors(db: Db, primaryNodeId: string): string[] {
  // One-hop: from_node rows where to_node = primaryNodeId.
  // For multi-hop, query data_flow_transitive or use the data_flow_reaches DSL relation.
  return db
    .select({ fromNode: dataFlow.fromNode })
    .from(dataFlow)
    .where(eq(dataFlow.toNode, primaryNodeId))
    .all()
    .map((r) => r.fromNode);
}

function queryDataFlowDescendants(db: Db, primaryNodeId: string): string[] {
  // One-hop: to_node rows where from_node = primaryNodeId.
  return db
    .select({ toNode: dataFlow.toNode })
    .from(dataFlow)
    .where(eq(dataFlow.fromNode, primaryNodeId))
    .all()
    .map((r) => r.toNode);
}

function queryDominanceRegion(db: Db, primaryNodeId: string): string[] {
  // Nodes dominated BY primaryNode.
  return db
    .select({ dominated: dominance.dominated })
    .from(dominance)
    .where(eq(dominance.dominator, primaryNodeId))
    .all()
    .map((r) => r.dominated);
}

function queryPostDominanceRegion(db: Db, primaryNodeId: string): string[] {
  // Nodes post-dominated BY primaryNode.
  return db
    .select({ postDominated: postDominance.postDominated })
    .from(postDominance)
    .where(eq(postDominance.postDominator, primaryNodeId))
    .all()
    .map((r) => r.postDominated);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Resolve a BugSignal's code references to a structural BugLocus via SAST queries.
 * Returns null if the signal has no resolvable code references.
 *
 * If multiple references resolve, picks the highest-confidence one. Ties break by
 * deepest (most-specific) non-leaf node covering the referenced line.
 * Confidence: 1.0 exact file+line, 0.8 file+function (no line), 0.3 file-only.
 */
export function locate(db: Db, signal: BugSignal): BugLocus | null {
  const candidates = resolveCandidates(db, signal);
  const primary = pickPrimary(candidates);
  if (!primary) return null;

  // Confidence from match kind
  const confidence =
    primary.matchKind === "line" ? 1.0 : primary.matchKind === "function" ? 0.8 : 0.3;

  // Get file root for fallback
  const fileRootId = primary.fileRow.rootNodeId;

  // Step 3: containingFunction
  // If primary IS a function-like node, it contains itself.
  const primaryNodeRow = db
    .select({ kind: nodes.kind })
    .from(nodes)
    .where(eq(nodes.id, primary.nodeId))
    .get();
  const primaryIsFunction =
    primaryNodeRow && FUNCTION_KINDS.has(primaryNodeRow.kind);
  const containingFunction = primaryIsFunction
    ? primary.nodeId
    : findContainingFunction(db, primary.nodeId, fileRootId);

  // Step 4: related functions (intra-file)
  const relatedFunctions = findRelatedFunctions(
    db,
    containingFunction,
    primary.fileRow.id,
  );

  // Steps 5–6: data flow (one-hop) + dominance
  const dataFlowAncestors = queryDataFlowAncestors(db, primary.nodeId);
  const dataFlowDescendants = queryDataFlowDescendants(db, primary.nodeId);
  const dominanceRegion = queryDominanceRegion(db, primary.nodeId);
  const postDominanceRegion = queryPostDominanceRegion(db, primary.nodeId);

  return {
    file: primary.ref.file,
    line: primary.ref.line ?? primary.sourceLine,
    function: primary.ref.function,
    confidence,
    primaryNode: primary.nodeId,
    containingFunction,
    relatedFunctions,
    dataFlowAncestors,
    dataFlowDescendants,
    dominanceRegion,
    postDominanceRegion,
  };
}
