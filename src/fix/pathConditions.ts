/**
 * pathConditions.ts
 *
 * Helper for oracle #2's guard-augmented SMT path.
 *
 * Walks the overlay SAST for IfStatement guards that structurally contain
 * the invariant's binding sites in their then/else branch. Extracts simple
 * literal-comparison guards as SMT assertions that constrain the bound
 * variables, so Z3 can determine whether the guard makes the invariant
 * violation unreachable.
 *
 * Key design note: The SAST dominance table stores statement-level dominance
 * only (FunctionDeclaration, IfStatement, ReturnStatement, etc.) — NOT
 * expression-level nodes like Identifier. So we:
 *   1. Walk the nodeChildren parent chain from the binding's use-site to
 *      find the enclosing statement node.
 *   2. Check that statement's dominators for IfStatement nodes (via decides).
 *   3. Determine branch direction (then vs else) by checking structural
 *      ancestry: is the enclosing statement a child (transitively) of
 *      the consequentNode or alternateNode Block?
 *
 * MVP scope:
 *   - decides rows where decision_kind == "if" with a BinaryExpression
 *     condition using ==, !=, ===, !==, >, <, >=, <= against a numeric literal
 *   - Subject of the guard must match a binding by source_expr string
 *   - Operator and literal recovered from condition source text
 *   - Graceful fallback: anything unrecognized returns empty arrays
 */

import { readFileSync, existsSync } from "fs";
import { join } from "path";
import { eq, inArray } from "drizzle-orm";
import { nodes, files, dominance, nodeDecides, nodeChildren, nodeReturns } from "../sast/schema/index.js";
import type { OverlayHandle } from "./types.js";
import type { SmtBinding } from "../contracts.js";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export interface ExtractedGuards {
  /** SMT fragments as (assert ...) statements, to be concatenated with the violation SMT. */
  smtAssertions: string[];
  /** Number of dominating guards found (before deduplication). */
  guardCount: number;
}

/**
 * Walk the overlay's SAST for guards dominating the invariant's binding sites.
 * Extract simple literal-comparison guards as SMT assertions that constrain
 * the bound variables further.
 *
 * MVP scope: numeric literal comparisons only. Falls back silently for
 * anything we can't SMT-ize.
 */
export function extractGuardConditions(
  overlay: OverlayHandle,
  bindings: SmtBinding[],
): ExtractedGuards {
  if (bindings.length === 0) {
    return { smtAssertions: [], guardCount: 0 };
  }

  const db = overlay.sastDb;
  const worktree = overlay.worktreePath;

  // Cache file contents keyed by absolute path.
  const fileContentCache = new Map<string, string>();

  function readFile(absPath: string): string | null {
    if (fileContentCache.has(absPath)) return fileContentCache.get(absPath)!;
    if (!existsSync(absPath)) return null;
    try {
      const content = readFileSync(absPath, "utf-8");
      fileContentCache.set(absPath, content);
      return content;
    } catch {
      return null;
    }
  }

  // Build a mapping: source_expr → SmtBinding (first match wins)
  const bindingByExpr = new Map<string, SmtBinding>();
  for (const b of bindings) {
    if (!bindingByExpr.has(b.source_expr)) {
      bindingByExpr.set(b.source_expr, b);
    }
  }

  // -------------------------------------------------------------------------
  // Step 1: Load file contents + all nodes in the overlay DB.
  //   Build:
  //     nodeId → { sourceStart, sourceEnd, fileId, kind }
  //     fileId → absPath + content
  // -------------------------------------------------------------------------

  const allFiles = db.select({
    id: files.id,
    path: files.path,
  }).from(files).all();

  const fileAbsPath = new Map<number, string>();
  const fileContent = new Map<number, string>();

  for (const fileRow of allFiles) {
    const absPath = fileRow.path.startsWith("/") ? fileRow.path : join(worktree, fileRow.path);
    fileAbsPath.set(fileRow.id, absPath);
    const content = readFile(absPath);
    if (content !== null) {
      fileContent.set(fileRow.id, content);
    }
  }

  const allNodes = db.select({
    id: nodes.id,
    fileId: nodes.fileId,
    sourceStart: nodes.sourceStart,
    sourceEnd: nodes.sourceEnd,
    kind: nodes.kind,
  }).from(nodes).all();

  const nodeById = new Map<string, typeof allNodes[0]>();
  for (const n of allNodes) {
    nodeById.set(n.id, n);
  }

  // -------------------------------------------------------------------------
  // Step 2: Find nodes whose source text matches a binding expression.
  //   Map: nodeId → SmtBinding
  // -------------------------------------------------------------------------

  const nodeToBinding = new Map<string, SmtBinding>();

  for (const n of allNodes) {
    const content = fileContent.get(n.fileId);
    if (!content) continue;
    // sourceStart = getFullStart() which includes leading whitespace trivia.
    // Trim to get the actual token text for comparison against source_expr.
    const text = content.slice(n.sourceStart, n.sourceEnd).trim();
    const binding = bindingByExpr.get(text);
    if (binding && !nodeToBinding.has(n.id)) {
      nodeToBinding.set(n.id, binding);
    }
  }

  if (nodeToBinding.size === 0) {
    return { smtAssertions: [], guardCount: 0 };
  }

  // -------------------------------------------------------------------------
  // Step 3: Build parent→child and child→parent maps from nodeChildren.
  //   We'll use these to walk up the tree to find enclosing statements.
  // -------------------------------------------------------------------------

  const allChildren = db.select({
    parentId: nodeChildren.parentId,
    childId: nodeChildren.childId,
  }).from(nodeChildren).all();

  // child → parent map
  const parentOf = new Map<string, string>();
  for (const row of allChildren) {
    parentOf.set(row.childId, row.parentId);
  }

  // parent → set of children
  const childrenOf = new Map<string, Set<string>>();
  for (const row of allChildren) {
    if (!childrenOf.has(row.parentId)) {
      childrenOf.set(row.parentId, new Set());
    }
    childrenOf.get(row.parentId)!.add(row.childId);
  }

  // -------------------------------------------------------------------------
  // Step 4: For each use-site node, find its enclosing statement.
  //   Walk up parentOf until we reach a statement-level node (one that
  //   appears in the dominance table as a dominated node, or a known
  //   statement kind).
  //
  //   The dominance table only contains statement-level nodes, so we can
  //   use "has a dominated entry" as the statement indicator.
  // -------------------------------------------------------------------------

  // Build set of all nodes that appear as "dominated" in the dominance table.
  const allDominanceRows = db.select({
    dominator: dominance.dominator,
    dominated: dominance.dominated,
  }).from(dominance).all();

  const statementNodeIds = new Set<string>();
  for (const row of allDominanceRows) {
    statementNodeIds.add(row.dominated);
    statementNodeIds.add(row.dominator);
  }

  // Map: dominated_nodeId → Set<dominator_nodeId>
  const dominatorsOf = new Map<string, Set<string>>();
  for (const row of allDominanceRows) {
    if (!dominatorsOf.has(row.dominated)) {
      dominatorsOf.set(row.dominated, new Set());
    }
    dominatorsOf.get(row.dominated)!.add(row.dominator);
  }

  /**
   * Walk up the parent chain from nodeId until we find a node that appears
   * in the dominance table (i.e., a statement-level node).
   * Returns null if none found within 20 steps.
   */
  function findEnclosingStatement(nodeId: string): string | null {
    let current = nodeId;
    for (let i = 0; i < 20; i++) {
      if (statementNodeIds.has(current)) return current;
      const parent = parentOf.get(current);
      if (!parent) return null;
      current = parent;
    }
    return null;
  }

  // Map: use-site nodeId → enclosing statement nodeId
  const useSiteToStatement = new Map<string, string>();
  for (const useSiteId of nodeToBinding.keys()) {
    const stmt = findEnclosingStatement(useSiteId);
    if (stmt) {
      useSiteToStatement.set(useSiteId, stmt);
    }
  }

  if (useSiteToStatement.size === 0) {
    return { smtAssertions: [], guardCount: 0 };
  }

  // -------------------------------------------------------------------------
  // Step 5: Find which IfStatement nodes are dominators of the enclosing
  //   statements. Query decides table.
  // -------------------------------------------------------------------------

  const allDecides = db.select({
    nodeId: nodeDecides.nodeId,
    conditionNode: nodeDecides.conditionNode,
    consequentNode: nodeDecides.consequentNode,
    alternateNode: nodeDecides.alternateNode,
    decisionKind: nodeDecides.decisionKind,
  }).from(nodeDecides).all();

  const decidesById = new Map<string, typeof allDecides[0]>();
  for (const row of allDecides) {
    if (row.decisionKind === "if") {
      decidesById.set(row.nodeId, row);
    }
  }

  if (decidesById.size === 0) {
    return { smtAssertions: [], guardCount: 0 };
  }

  // -------------------------------------------------------------------------
  // Step 5b: Load all exit-kind rows from node_returns for early-exit detection.
  // -------------------------------------------------------------------------

  const allReturns = db.select({ nodeId: nodeReturns.nodeId }).from(nodeReturns).all();
  const exitNodeIds = new Set(allReturns.map((r) => r.nodeId));

  // -------------------------------------------------------------------------
  // Step 6: Build structural ancestry sets for consequentNode and alternateNode.
  //
  //   For each IfStatement, we need to know which nodes are structurally
  //   inside the then-block (consequentNode) vs else-block (alternateNode).
  //   We do a BFS from each branch block node using childrenOf.
  //
  //   This is used to determine polarity: if the enclosing statement is
  //   inside the then-block, use condition as-is; if inside else-block, negate.
  // -------------------------------------------------------------------------

  function collectDescendants(rootId: string): Set<string> {
    const result = new Set<string>();
    const queue = [rootId];
    while (queue.length > 0) {
      const curr = queue.pop()!;
      if (result.has(curr)) continue;
      result.add(curr);
      const children = childrenOf.get(curr);
      if (children) {
        for (const child of children) queue.push(child);
      }
    }
    return result;
  }

  // Precompute descendants for all branch blocks.
  const descendantsOf = new Map<string, Set<string>>();
  for (const decides of decidesById.values()) {
    if (decides.consequentNode && !descendantsOf.has(decides.consequentNode)) {
      descendantsOf.set(decides.consequentNode, collectDescendants(decides.consequentNode));
    }
    if (decides.alternateNode && !descendantsOf.has(decides.alternateNode)) {
      descendantsOf.set(decides.alternateNode, collectDescendants(decides.alternateNode));
    }
  }

  /**
   * Returns true if the branch rooted at branchNode unconditionally exits
   * (i.e., any descendant — including branchNode itself — is in exitNodeIds).
   *
   * Note: collectDescendants includes rootId itself, so braceless
   * `if (c) throw ...` is handled correctly.
   */
  function branchAlwaysExits(branchNode: string): boolean {
    const descendants = descendantsOf.get(branchNode);
    if (!descendants) return false;
    for (const d of descendants) {
      if (exitNodeIds.has(d)) return true;
    }
    return false;
  }

  // -------------------------------------------------------------------------
  // Step 7: Fetch condition node positions.
  // -------------------------------------------------------------------------

  const conditionNodeIds = [...decidesById.values()].map((r) => r.conditionNode);

  const conditionNodeRows = db.select({
    id: nodes.id,
    sourceStart: nodes.sourceStart,
    sourceEnd: nodes.sourceEnd,
    fileId: nodes.fileId,
  }).from(nodes)
    .where(inArray(nodes.id, conditionNodeIds))
    .all();

  const conditionNodeById = new Map<string, typeof conditionNodeRows[0]>();
  for (const row of conditionNodeRows) {
    conditionNodeById.set(row.id, row);
  }

  // -------------------------------------------------------------------------
  // Step 8: Assemble SMT assertions.
  //   For each use-site:
  //     - Find enclosing statement
  //     - Find its IfStatement dominators
  //     - Check if enclosing statement is inside consequent (then) or
  //       alternate (else) block via structural ancestry
  //     - Parse condition text → build SMT assertion
  // -------------------------------------------------------------------------

  const smtAssertions: string[] = [];
  const seen = new Set<string>();
  let guardCount = 0;

  for (const [useSiteId, binding] of nodeToBinding) {
    const stmtId = useSiteToStatement.get(useSiteId);
    if (!stmtId) continue;

    const stmtDominators = dominatorsOf.get(stmtId);
    if (!stmtDominators) continue;

    for (const dominatorId of stmtDominators) {
      // Skip self-dominance.
      if (dominatorId === stmtId) continue;

      const decides = decidesById.get(dominatorId);
      if (!decides) continue;

      // Determine branch direction via structural ancestry.
      const thenDescendants = decides.consequentNode
        ? descendantsOf.get(decides.consequentNode)
        : undefined;
      const elseDescendants = decides.alternateNode
        ? descendantsOf.get(decides.alternateNode)
        : undefined;

      // The enclosing statement must be a structural descendant of EXACTLY ONE branch,
      // OR it may be AFTER the if (early-exit guard pattern).
      const inConsequent = thenDescendants !== undefined && thenDescendants.has(stmtId);
      const inAlternate = elseDescendants !== undefined && elseDescendants.has(stmtId);

      let negate: boolean;
      if (inConsequent && !inAlternate) {
        negate = false;
      } else if (inAlternate && !inConsequent) {
        negate = true;
      } else if (!inConsequent && !inAlternate) {
        // Early-exit guard pattern: use-site is AFTER the if-statement, and the
        // then-branch always exits. Reaching the use-site implies the condition was false.
        const consNode = decides.consequentNode;
        if (!consNode || !branchAlwaysExits(consNode)) continue;
        const useStart = nodeById.get(stmtId)?.sourceStart;
        const ifStart = nodeById.get(dominatorId)?.sourceStart;
        if (useStart == null || ifStart == null || useStart <= ifStart) continue;
        negate = true;
      } else {
        continue; // inConsequent && inAlternate = ambiguous
      }

      // Fetch condition text.
      const condNode = conditionNodeById.get(decides.conditionNode);
      if (!condNode) continue;

      const filePath = fileAbsPath.get(condNode.fileId);
      if (!filePath) continue;

      const content = readFile(filePath);
      if (!content) continue;

      const condText = content.slice(condNode.sourceStart, condNode.sourceEnd).trim();

      // Parse condition and build SMT.
      const smt = buildSmtFromCondition(condText, binding, negate);
      if (!smt) continue;

      const dedupeKey = `${binding.smt_constant}:${smt}`;
      if (seen.has(dedupeKey)) continue;
      seen.add(dedupeKey);

      smtAssertions.push(smt);
      guardCount++;
    }
  }

  return { smtAssertions, guardCount };
}

// ---------------------------------------------------------------------------
// SMT builder for a single condition
// ---------------------------------------------------------------------------

/**
 * Parse a condition text like "b !== 0" or "b === null" and build an SMT
 * assertion that constrains `binding.smt_constant`.
 *
 * MVP scope:
 *   - Operators: ==, !=, ===, !==, >, <, >=, <=
 *   - RHS/LHS must be a numeric literal, null, or undefined
 *   - LHS or RHS must text-match binding.source_expr
 *
 * Returns null if the condition is not in MVP scope.
 */
function buildSmtFromCondition(
  condText: string,
  binding: SmtBinding,
  negate: boolean,
): string | null {
  // Supported operators in order (longer first to avoid prefix ambiguity).
  const ops = ["!==", "===", "!=", "==", ">=", "<=", ">", "<"] as const;

  for (const op of ops) {
    const idx = condText.indexOf(op);
    if (idx === -1) continue;

    const lhsText = condText.slice(0, idx).trim();
    const rhsText = condText.slice(idx + op.length).trim();

    // Determine which side is the binding expression and which is the literal.
    let literalText: string;
    let flipOp = false;

    if (lhsText === binding.source_expr) {
      literalText = rhsText;
    } else if (rhsText === binding.source_expr) {
      literalText = lhsText;
      flipOp = true;
    } else {
      continue;
    }

    // Parse the literal.
    const smtLiteral = parseLiteral(literalText, binding.sort);
    if (smtLiteral === null) continue;

    // Build the SMT comparison.
    const smtVar = binding.smt_constant;
    const effectiveOp = flipOp ? flipOperator(op) : op;
    const smtComparison = buildSmtComparison(smtVar, effectiveOp, smtLiteral);
    if (smtComparison === null) continue;

    const assertion = negate
      ? `(assert (not ${smtComparison}))`
      : `(assert ${smtComparison})`;

    return assertion;
  }

  return null;
}

/**
 * Parse a literal text to an SMT literal string.
 */
function parseLiteral(text: string, sort: string): string | null {
  if (/^-?\d+(\.\d+)?$/.test(text)) {
    if (sort === "Int" && text.includes(".")) return null;
    return text;
  }
  if (text === "null" || text === "undefined") {
    if (sort === "Int") return "0";
    if (sort === "Real") return "0.0";
    return null;
  }
  return null;
}

/**
 * Flip an operator when the literal is on the left side.
 */
function flipOperator(op: string): string {
  switch (op) {
    case ">": return "<";
    case "<": return ">";
    case ">=": return "<=";
    case "<=": return ">=";
    default: return op;
  }
}

/**
 * Build the SMT comparison s-expr (not wrapped in assert).
 */
function buildSmtComparison(
  smtVar: string,
  op: string,
  smtLiteral: string,
): string | null {
  switch (op) {
    case "==":
    case "===":
      return `(= ${smtVar} ${smtLiteral})`;
    case "!=":
    case "!==":
      return `(not (= ${smtVar} ${smtLiteral}))`;
    case ">":
      return `(> ${smtVar} ${smtLiteral})`;
    case "<":
      return `(< ${smtVar} ${smtLiteral})`;
    case ">=":
      return `(>= ${smtVar} ${smtLiteral})`;
    case "<=":
      return `(<= ${smtVar} ${smtLiteral})`;
    default:
      return null;
  }
}
