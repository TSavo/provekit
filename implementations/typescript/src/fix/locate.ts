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

// Path subtrees that are unlikely to be real patch targets. When a candidate's
// resolved file path contains any of these segments, it is penalized in the
// score below — vendored/built/reference code is rarely where the bug lives,
// even if the LLM names it as a candidate. Match against path segments
// (slash-bounded) so this stays robust to substring false positives like
// `/var/folders/...` on macOS tmpdirs.
const NON_SOURCE_SEGMENTS = [
  "reference",
  "vendor",
  "node_modules",
  "dist",
  "build",
];

function isInNonSourceSubtree(path: string): boolean {
  const segments = path.split("/");
  for (const seg of NON_SOURCE_SEGMENTS) {
    if (segments.includes(seg)) return true;
  }
  return false;
}

function investigateConfidenceTier(
  c: "high" | "medium" | "low" | undefined,
): number {
  // 3 = high, 2 = medium, 1 = low, 0 = no Investigate signal at all.
  // The 0 value matters: Intake-supplied refs (corpus, harvest, recognize)
  // must not be ranked BELOW Investigate-low refs, only equal-tier-and-down.
  if (c === "high") return 3;
  if (c === "medium") return 2;
  if (c === "low") return 1;
  return 0;
}

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
  // Threaded from CodeReference for scoring. Carried on the Candidate
  // (rather than re-derived inside matchScore) so resolveCandidates can
  // compute path-shape once per ref.
  investigateConfidence: "high" | "medium" | "low" | undefined;
  isPrimary: boolean;
  /** True if the RESOLVED file path sits in a non-source subtree (reference/, vendor/, etc.). */
  inNonSourceSubtree: boolean;
}

function resolveCandidates(db: Db, signal: BugSignal): Candidate[] {
  const all: Candidate[] = [];

  for (const ref of signal.codeReferences) {
    const fileRow = resolveFile(db, ref.file);
    if (!fileRow) continue;

    // Score-shaping metadata is fixed for every Candidate produced from this
    // ref: the resolved file path determines subtree-shape, and the upstream
    // CodeReference carries the Investigate signals.
    const refMeta = {
      investigateConfidence: ref.investigateConfidence,
      isPrimary: ref.isPrimary === true,
      inNonSourceSubtree: isInNonSourceSubtree(fileRow.path),
    };

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
          ...refMeta,
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
          ...refMeta,
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
            ...refMeta,
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
          ...refMeta,
        });
      }
    }
  }

  return all;
}

// ---------------------------------------------------------------------------
// Step 3: Pick the primary node
// ---------------------------------------------------------------------------

function matchScore(c: Candidate): number[] {
  // Higher is better at every position. Comparison is lexicographic — earlier
  // positions dominate later ones — so the tuple ORDERING expresses policy.
  //
  // Position 0: resolved-high-confidence-primary boost. When Investigate
  //   marks a ref as `isPrimary` AND assigns it `high` confidence AND the
  //   ref resolved in the substrate (we wouldn't be here otherwise), it
  //   wins outright. This is rule (1) of task #145: prefer Investigate's
  //   primary when it has high confidence and resolves.
  //
  // Position 1: investigate confidence tier (high=3, medium=2, low=1, none=0).
  //   This is the fallback ordering when there is no high-confidence primary
  //   among the resolved candidates — a medium primary still beats a low
  //   candidate, etc. Rule (2): candidate locations are fallbacks.
  //
  // Position 2: non-source-subtree penalty. Candidates whose resolved path
  //   sits in `reference/`, `vendor/`, `node_modules/`, `dist/`, or `build/`
  //   score 0 here while real-source candidates score 1. This is rule (3):
  //   penalize subtrees that are unlikely patch targets.
  //
  // Position 3: precision tier (line=2, function=1, file=0). The original
  //   B2 ranking — prefer line refs over function-name refs over file-only.
  //
  // Position 4: non-leaf bonus (BinaryExpression beats Identifier on the
  //   same line).
  //
  // Position 5: negative span (smaller node beats larger node). Existing
  //   B2 tiebreak.
  const isHighPrimary = c.isPrimary && c.investigateConfidence === "high" ? 1 : 0;
  const investigateTier = investigateConfidenceTier(c.investigateConfidence);
  const sourceSubtreeBonus = c.inNonSourceSubtree ? 0 : 1;
  const precisionTier = c.matchKind === "line" ? 2 : c.matchKind === "function" ? 1 : 0;
  const leafPenalty = c.isLeaf ? 0 : 1;
  return [isHighPrimary, investigateTier, sourceSubtreeBonus, precisionTier, leafPenalty, -c.span];
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
 * Ranking, in priority order (see matchScore in this file):
 *   1. Investigate's primary IF marked `high` confidence and resolved.
 *   2. Investigate confidence tier (high > medium > low > none).
 *   3. Source-subtree preference: paths in `reference/`, `vendor/`,
 *      `node_modules/`, `dist/`, `build/` are penalized.
 *   4. Precision tier: line > function > file-only.
 *   5. Non-leaf preference: BinaryExpression beats Identifier on the same line.
 *   6. Smaller span beats larger span (most-specific node).
 *
 * Confidence: 1.0 exact file+line, 0.8 file+function (no line), 0.3 file-only.
 *
 * Rule (4) of task #145 — "when Locate's confidence in any candidate is below
 * 0.5 AND a higher-confidence Investigate primary exists, use the primary
 * anyway" — falls out of the ranking automatically: `isPrimary && high` is
 * the top dimension, so a high-confidence primary always wins over any
 * file-only (0.3) candidate even if the primary itself is file-only.
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

  // Look up the containing function's binding name so we can populate
  // locus.function even when the caller only supplied file+line (no
  // ref.function). Skip when the "containing function" is the file root —
  // module-top-level has no enclosing function name to report.
  const containingFunctionName: string | undefined = (() => {
    if (containingFunction === fileRootId) return undefined;
    try {
      const row = db
        .select({ name: nodeBinding.name })
        .from(nodeBinding)
        .where(eq(nodeBinding.nodeId, containingFunction))
        .get();
      return row?.name;
    } catch {
      return undefined;
    }
  })();

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
    function: primary.ref.function ?? containingFunctionName,
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
