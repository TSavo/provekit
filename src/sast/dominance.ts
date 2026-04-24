/**
 * A5: Dominance + post-dominance via CFG + iterative dataflow.
 *
 * For each function-like node in the file, build an approximate CFG over
 * statement-level nodes, compute standard iterative dominance, then reverse
 * the CFG for post-dominance. Materialize transitive closures into the
 * `dominance` and `post_dominance` tables.
 *
 * Design decisions:
 * - Block is transparent: edges skip Blocks and attach to their first child
 *   statement directly. This keeps chains clean.
 * - ENTRY = the function node itself (already in nodeIdByNode).
 * - EXIT = virtual pseudo-node (string constant). Never inserted in DB.
 * - Only reachable nodes (DFS from ENTRY) participate in Dom computation.
 * - Per-function AllNodes set — no cross-function pollution.
 * - No exception edges (v1 scope). Catch clauses are unreachable.
 * - Deduplication via in-memory Set before insertion — no ON CONFLICT.
 */

import {
  SyntaxKind,
  type SourceFile,
  type Node,
  type Block,
  type Statement,
  type IfStatement,
  type WhileStatement,
  type ForStatement,
  type ForOfStatement,
  type ForInStatement,
  type DoStatement,
  type SwitchStatement,
  type TryStatement,
  type FunctionDeclaration,
  type FunctionExpression,
  type ArrowFunction,
  type MethodDeclaration,
  type GetAccessorDeclaration,
  type SetAccessorDeclaration,
  type ConstructorDeclaration,
} from "ts-morph";
import type { SastTx } from "./builder.js";
import { dominance, postDominance } from "./schema/index.js";
import type { NodeIdMap } from "./capabilities/extractor.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const EXIT = "__EXIT__";

// Function-like SyntaxKinds for which we build a CFG
const FUNCTION_KINDS = new Set([
  SyntaxKind.FunctionDeclaration,
  SyntaxKind.FunctionExpression,
  SyntaxKind.ArrowFunction,
  SyntaxKind.MethodDeclaration,
  SyntaxKind.GetAccessor,
  SyntaxKind.SetAccessor,
  SyntaxKind.Constructor,
]);

// Statement kinds that are terminals (always go to EXIT)
const TERMINAL_KINDS = new Set([
  SyntaxKind.ReturnStatement,
  SyntaxKind.ThrowStatement,
]);

// ---------------------------------------------------------------------------
// CFG types
// ---------------------------------------------------------------------------

/** node ID → set of successor node IDs (including EXIT as string) */
type Adj = Map<string, Set<string>>;

// ---------------------------------------------------------------------------
// Block transparency
// ---------------------------------------------------------------------------

/**
 * If node is a Block, return its first statement; otherwise return node.
 * Used to skip Block wrappers when connecting CFG edges.
 */
function firstOfBlock(node: Node): Node | null {
  if (node.getKind() === SyntaxKind.Block) {
    const stmts = (node as Block).getStatements();
    if (stmts.length === 0) return null;
    return stmts[0];
  }
  return node;
}

/**
 * Return the sequence of statements in a Block-like container.
 * If node is a Block, returns its statements. Otherwise returns [node].
 */
function statementsOf(node: Node): Statement[] {
  if (node.getKind() === SyntaxKind.Block) {
    return (node as Block).getStatements();
  }
  return [node as Statement];
}

// ---------------------------------------------------------------------------
// CFG construction
// ---------------------------------------------------------------------------

/**
 * Connect edges for a sequence of statements within a block.
 * `exitTarget` is where to go after the last statement (parent's successor).
 */
function connectSequence(
  stmts: Statement[],
  exitTarget: string,
  adj: Adj,
  nodeIdByNode: NodeIdMap,
): void {
  for (let i = 0; i < stmts.length; i++) {
    const stmt = stmts[i];
    const next = i + 1 < stmts.length ? stmts[i + 1] : null;
    const nextTarget = next ? (nodeIdByNode.get(next) ?? exitTarget) : exitTarget;
    addStmtEdges(stmt, nextTarget, adj, nodeIdByNode);
  }
}

/**
 * Add CFG edges for a single statement, given its successor target.
 */
function addStmtEdges(
  stmt: Statement,
  nextTarget: string,
  adj: Adj,
  nodeIdByNode: NodeIdMap,
): void {
  const id = nodeIdByNode.get(stmt);
  if (!id) return; // node not registered — skip

  if (!adj.has(id)) adj.set(id, new Set());

  const kind = stmt.getKind();

  if (TERMINAL_KINDS.has(kind)) {
    // return / throw → EXIT
    adj.get(id)!.add(EXIT);
    return;
  }

  if (kind === SyntaxKind.IfStatement) {
    const ifStmt = stmt as IfStatement;
    const thenStmt = ifStmt.getThenStatement();
    const elseStmt = ifStmt.getElseStatement();

    // if → then-branch (skip Block wrapper)
    const thenFirst = firstOfBlock(thenStmt);
    const thenTarget = thenFirst ? (nodeIdByNode.get(thenFirst) ?? nextTarget) : nextTarget;
    adj.get(id)!.add(thenTarget);

    // if → else-branch or fallthrough
    if (elseStmt) {
      const elseFirst = firstOfBlock(elseStmt);
      const elseTarget = elseFirst ? (nodeIdByNode.get(elseFirst) ?? nextTarget) : nextTarget;
      adj.get(id)!.add(elseTarget);
    } else {
      adj.get(id)!.add(nextTarget);
    }

    // Wire then-branch statements internally
    const thenStmts = statementsOf(thenStmt);
    for (let i = 0; i < thenStmts.length; i++) {
      const s = thenStmts[i];
      const nxt = i + 1 < thenStmts.length
        ? (nodeIdByNode.get(thenStmts[i + 1]) ?? nextTarget)
        : nextTarget;
      addStmtEdges(s, nxt, adj, nodeIdByNode);
    }

    // Wire else-branch statements internally
    if (elseStmt) {
      const elseStmts = statementsOf(elseStmt);
      for (let i = 0; i < elseStmts.length; i++) {
        const s = elseStmts[i];
        const nxt = i + 1 < elseStmts.length
          ? (nodeIdByNode.get(elseStmts[i + 1]) ?? nextTarget)
          : nextTarget;
        addStmtEdges(s, nxt, adj, nodeIdByNode);
      }
    }
    return;
  }

  if (kind === SyntaxKind.WhileStatement) {
    const ws = stmt as WhileStatement;
    const body = ws.getStatement();
    const bodyStmts = statementsOf(body);

    // loop-header → first body statement (or back to header if body is empty)
    if (bodyStmts.length > 0) {
      const firstBody = nodeIdByNode.get(bodyStmts[0]);
      if (firstBody) adj.get(id)!.add(firstBody);
    }
    // loop-header → exit
    adj.get(id)!.add(nextTarget);

    // body last statement → loop-header (backedge)
    for (let i = 0; i < bodyStmts.length; i++) {
      const s = bodyStmts[i];
      const nxt = i + 1 < bodyStmts.length
        ? (nodeIdByNode.get(bodyStmts[i + 1]) ?? id)
        : id; // last body stmt → header
      addStmtEdges(s, nxt, adj, nodeIdByNode);
    }
    return;
  }

  if (kind === SyntaxKind.ForStatement) {
    const fs = stmt as ForStatement;
    const body = fs.getStatement();
    const bodyStmts = statementsOf(body);

    // for-header → first body statement
    if (bodyStmts.length > 0) {
      const firstBody = nodeIdByNode.get(bodyStmts[0]);
      if (firstBody) adj.get(id)!.add(firstBody);
    }
    // for-header → exit
    adj.get(id)!.add(nextTarget);

    // body last → header
    for (let i = 0; i < bodyStmts.length; i++) {
      const s = bodyStmts[i];
      const nxt = i + 1 < bodyStmts.length
        ? (nodeIdByNode.get(bodyStmts[i + 1]) ?? id)
        : id;
      addStmtEdges(s, nxt, adj, nodeIdByNode);
    }
    return;
  }

  if (kind === SyntaxKind.ForOfStatement || kind === SyntaxKind.ForInStatement) {
    const fo = stmt as ForOfStatement | ForInStatement;
    const body = fo.getStatement();
    const bodyStmts = statementsOf(body);

    if (bodyStmts.length > 0) {
      const firstBody = nodeIdByNode.get(bodyStmts[0]);
      if (firstBody) adj.get(id)!.add(firstBody);
    }
    adj.get(id)!.add(nextTarget);

    for (let i = 0; i < bodyStmts.length; i++) {
      const s = bodyStmts[i];
      const nxt = i + 1 < bodyStmts.length
        ? (nodeIdByNode.get(bodyStmts[i + 1]) ?? id)
        : id;
      addStmtEdges(s, nxt, adj, nodeIdByNode);
    }
    return;
  }

  if (kind === SyntaxKind.DoStatement) {
    const ds = stmt as DoStatement;
    const body = ds.getStatement();
    const bodyStmts = statementsOf(body);

    // do-header → first body statement (body runs at least once)
    if (bodyStmts.length > 0) {
      const firstBody = nodeIdByNode.get(bodyStmts[0]);
      if (firstBody) adj.get(id)!.add(firstBody);
    }
    // do-header → exit (condition check can fail)
    adj.get(id)!.add(nextTarget);

    // body last → header (backedge, for re-checking condition)
    for (let i = 0; i < bodyStmts.length; i++) {
      const s = bodyStmts[i];
      const nxt = i + 1 < bodyStmts.length
        ? (nodeIdByNode.get(bodyStmts[i + 1]) ?? id)
        : id;
      addStmtEdges(s, nxt, adj, nodeIdByNode);
    }
    return;
  }

  if (kind === SyntaxKind.SwitchStatement) {
    const sw = stmt as SwitchStatement;
    const clauses = sw.getCaseBlock().getClauses();

    // switch → each case first statement (conservative: branch to all)
    let prevHasFallthrough = false;
    let prevLastId: string | null = null;

    for (let ci = 0; ci < clauses.length; ci++) {
      const clause = clauses[ci];
      const clauseStmts = clause.getStatements();

      if (clauseStmts.length === 0) {
        // Empty clause — fallthrough directly
        // No node to add — mark for fallthrough
        prevHasFallthrough = true;
        continue;
      }

      const firstClauseId = nodeIdByNode.get(clauseStmts[0]);
      if (firstClauseId) {
        // switch → this case's first stmt
        adj.get(id)!.add(firstClauseId);

        // if previous clause had fallthrough, connect it here
        if (prevHasFallthrough && prevLastId) {
          if (!adj.has(prevLastId)) adj.set(prevLastId, new Set());
          adj.get(prevLastId)!.add(firstClauseId);
        }
      }

      // Wire clause statements internally
      for (let i = 0; i < clauseStmts.length; i++) {
        const s = clauseStmts[i];
        const sKind = s.getKind();

        if (sKind === SyntaxKind.BreakStatement) {
          const sId = nodeIdByNode.get(s);
          if (sId) {
            if (!adj.has(sId)) adj.set(sId, new Set());
            adj.get(sId)!.add(nextTarget);
          }
        } else {
          const nxt = i + 1 < clauseStmts.length
            ? (nodeIdByNode.get(clauseStmts[i + 1]) ?? nextTarget)
            : nextTarget; // last stmt of case → after switch (unless it's a break/terminal)
          addStmtEdges(s, nxt, adj, nodeIdByNode);
        }
      }

      // Check if last statement is terminal (break/return/throw) → no fallthrough
      const lastStmt = clauseStmts[clauseStmts.length - 1];
      const lastKind = lastStmt.getKind();
      const isTerminal = lastKind === SyntaxKind.BreakStatement ||
        TERMINAL_KINDS.has(lastKind);

      prevHasFallthrough = !isTerminal;
      const lastId = nodeIdByNode.get(lastStmt);
      prevLastId = lastId ?? null;
    }

    // After switch → nextTarget (for fall-through cases or empty switch)
    adj.get(id)!.add(nextTarget);
    return;
  }

  if (kind === SyntaxKind.TryStatement) {
    const ts = stmt as TryStatement;
    const tryBlock = ts.getTryBlock();
    const finallyBlock = ts.getFinallyBlock();
    // Catch is unreachable in v1 — skip it

    const tryStmts = tryBlock.getStatements();

    // After the try block, either go to finally or nextTarget
    const afterTryTarget = finallyBlock
      ? (() => {
          const finallyStmts = finallyBlock.getStatements();
          if (finallyStmts.length > 0) {
            return nodeIdByNode.get(finallyStmts[0]) ?? nextTarget;
          }
          return nextTarget;
        })()
      : nextTarget;

    // try → first stmt in try-block
    if (tryStmts.length > 0) {
      const firstTry = nodeIdByNode.get(tryStmts[0]);
      if (firstTry) adj.get(id)!.add(firstTry);
      // Wire try block statements
      for (let i = 0; i < tryStmts.length; i++) {
        const s = tryStmts[i];
        const nxt = i + 1 < tryStmts.length
          ? (nodeIdByNode.get(tryStmts[i + 1]) ?? afterTryTarget)
          : afterTryTarget;
        addStmtEdges(s, nxt, adj, nodeIdByNode);
      }
    } else {
      adj.get(id)!.add(afterTryTarget);
    }

    // Wire finally block if present
    if (finallyBlock) {
      const finallyStmts = finallyBlock.getStatements();
      for (let i = 0; i < finallyStmts.length; i++) {
        const s = finallyStmts[i];
        const nxt = i + 1 < finallyStmts.length
          ? (nodeIdByNode.get(finallyStmts[i + 1]) ?? nextTarget)
          : nextTarget;
        addStmtEdges(s, nxt, adj, nodeIdByNode);
      }
    }
    return;
  }

  // Default: normal statement → nextTarget
  adj.get(id)!.add(nextTarget);
}

/**
 * Get the body statements of a function-like node.
 * For ArrowFunction with expression body, wraps in a synthetic list.
 */
function getFunctionBodyStatements(fnNode: Node): Statement[] | null {
  const kind = fnNode.getKind();

  if (kind === SyntaxKind.ArrowFunction) {
    const af = fnNode as ArrowFunction;
    const body = af.getBody();
    if (!body) return null;
    if (body.getKind() === SyntaxKind.Block) {
      return (body as Block).getStatements();
    }
    // Expression body — treat as a single synthetic return
    // We can't get a Statement node for the expression body directly,
    // so we skip CFG for expression-body arrow functions (empty body case).
    return null;
  }

  let body: Block | undefined;
  if (kind === SyntaxKind.FunctionDeclaration) {
    body = (fnNode as FunctionDeclaration).getBody() as Block | undefined;
  } else if (kind === SyntaxKind.FunctionExpression) {
    body = (fnNode as FunctionExpression).getBody() as Block | undefined;
  } else if (kind === SyntaxKind.MethodDeclaration) {
    body = (fnNode as MethodDeclaration).getBody() as Block | undefined;
  } else if (kind === SyntaxKind.GetAccessor) {
    body = (fnNode as GetAccessorDeclaration).getBody() as Block | undefined;
  } else if (kind === SyntaxKind.SetAccessor) {
    body = (fnNode as SetAccessorDeclaration).getBody() as Block | undefined;
  } else if (kind === SyntaxKind.Constructor) {
    body = (fnNode as ConstructorDeclaration).getBody() as Block | undefined;
  }

  if (!body) return null;
  return body.getStatements();
}

/**
 * Build the CFG adjacency list for a single function.
 * Returns: { adj (forward CFG), entryId, exitNodes (IDs that → EXIT) }
 */
function buildCFG(
  fnNode: Node,
  nodeIdByNode: NodeIdMap,
): { adj: Adj; entryId: string } | null {
  const entryId = nodeIdByNode.get(fnNode);
  if (!entryId) return null;

  const stmts = getFunctionBodyStatements(fnNode);
  if (!stmts || stmts.length === 0) return null;

  const adj: Adj = new Map();
  adj.set(entryId, new Set());

  // ENTRY → first statement
  const firstStmt = stmts[0];
  const firstId = nodeIdByNode.get(firstStmt);
  if (firstId) {
    adj.get(entryId)!.add(firstId);
  } else {
    return null;
  }

  // Wire all statements
  connectSequence(stmts, EXIT, adj, nodeIdByNode);

  return { adj, entryId };
}

// ---------------------------------------------------------------------------
// Reachability (DFS from entry)
// ---------------------------------------------------------------------------

function reachableFrom(entryId: string, adj: Adj): Set<string> {
  const visited = new Set<string>();
  const stack = [entryId];
  while (stack.length > 0) {
    const n = stack.pop()!;
    if (visited.has(n)) continue;
    visited.add(n);
    if (n === EXIT) continue; // don't expand EXIT
    const succs = adj.get(n);
    if (succs) {
      for (const s of succs) {
        if (!visited.has(s)) stack.push(s);
      }
    }
  }
  visited.delete(EXIT);
  return visited;
}

// ---------------------------------------------------------------------------
// Reverse postorder
// ---------------------------------------------------------------------------

function reversePostorder(entryId: string, adj: Adj, reachable: Set<string>): string[] {
  const visited = new Set<string>();
  const postorder: string[] = [];

  function dfs(n: string): void {
    visited.add(n);
    const succs = adj.get(n);
    if (succs) {
      for (const s of succs) {
        if (s !== EXIT && reachable.has(s) && !visited.has(s)) {
          dfs(s);
        }
      }
    }
    postorder.push(n);
  }

  dfs(entryId);
  return postorder.reverse();
}

// ---------------------------------------------------------------------------
// Iterative dominance fixpoint
// ---------------------------------------------------------------------------

/**
 * Compute dominance: for each node n, Dom(n) = set of all dominators of n.
 * Returns Map<nodeId, Set<dominator>>.
 * Only operates over `reachable` nodes.
 */
function computeDominance(
  entryId: string,
  adj: Adj,
  reachable: Set<string>,
): Map<string, Set<string>> {
  // Build predecessor map (within reachable, excluding EXIT)
  const preds = new Map<string, Set<string>>();
  for (const n of reachable) {
    if (!preds.has(n)) preds.set(n, new Set());
    const succs = adj.get(n);
    if (succs) {
      for (const s of succs) {
        if (s !== EXIT && reachable.has(s)) {
          if (!preds.has(s)) preds.set(s, new Set());
          preds.get(s)!.add(n);
        }
      }
    }
  }

  const rpo = reversePostorder(entryId, adj, reachable);

  // Initialize
  const Dom = new Map<string, Set<string>>();
  Dom.set(entryId, new Set([entryId]));
  for (const n of reachable) {
    if (n !== entryId) {
      Dom.set(n, new Set(reachable)); // Dom(n) = AllNodes initially
    }
  }

  // Iterate to fixpoint
  let changed = true;
  while (changed) {
    changed = false;
    for (const n of rpo) {
      if (n === entryId) continue;
      const nodePreds = preds.get(n);

      let newDom: Set<string>;
      if (!nodePreds || nodePreds.size === 0) {
        // Unreachable or no preds — keep as full set (shouldn't happen for reachable)
        newDom = new Set(reachable);
      } else {
        // Intersection of Dom(p) for all preds p
        let first = true;
        newDom = new Set<string>();
        for (const p of nodePreds) {
          const domP = Dom.get(p);
          if (!domP) continue;
          if (first) {
            for (const d of domP) newDom.add(d);
            first = false;
          } else {
            // Intersect in place
            for (const d of newDom) {
              if (!domP.has(d)) newDom.delete(d);
            }
          }
        }
        // Add n itself
        newDom.add(n);
      }

      const oldDom = Dom.get(n)!;
      // Check if changed
      if (newDom.size !== oldDom.size || [...newDom].some((d) => !oldDom.has(d))) {
        Dom.set(n, newDom);
        changed = true;
      }
    }
  }

  return Dom;
}

// ---------------------------------------------------------------------------
// Post-dominance: reverse CFG
// ---------------------------------------------------------------------------

/**
 * Build reverse CFG: flip all edges, add EXIT → all-terminals edges.
 * EXIT is the "entry" of the reverse CFG.
 * Returns: { revAdj, exitNodes (the set of forward-exit nodes = reverse entry's neighbors) }
 */
function buildReverseCFG(
  adj: Adj,
  reachable: Set<string>,
): { revAdj: Adj; exitId: string } {
  const revAdj: Adj = new Map();

  // Initialize rev adjacency for all reachable nodes + EXIT
  for (const n of reachable) {
    revAdj.set(n, new Set());
  }
  revAdj.set(EXIT, new Set());

  // Reverse all edges
  for (const [from, succs] of adj) {
    if (!reachable.has(from) && from !== EXIT) continue;
    for (const to of succs) {
      if (to === EXIT) {
        // forward: from → EXIT  ⟹  reverse: EXIT → from
        revAdj.get(EXIT)!.add(from);
      } else if (reachable.has(to)) {
        // forward: from → to  ⟹  reverse: to → from
        if (!revAdj.has(to)) revAdj.set(to, new Set());
        revAdj.get(to)!.add(from);
      }
    }
  }

  return { revAdj, exitId: EXIT };
}

/**
 * Compute post-dominance on the reverse CFG.
 * `exitId` is the virtual EXIT node.
 * Returns Map<nodeId, Set<post-dominator>> for real (non-EXIT) nodes.
 */
function computePostDominance(
  revAdj: Adj,
  reachable: Set<string>,
): Map<string, Set<string>> {
  // Build pred map for reverse CFG (within reachable + EXIT)
  const preds = new Map<string, Set<string>>();
  for (const n of reachable) {
    if (!preds.has(n)) preds.set(n, new Set());
  }
  if (!preds.has(EXIT)) preds.set(EXIT, new Set());

  for (const [from, succs] of revAdj) {
    for (const to of succs) {
      if (to !== EXIT && !reachable.has(to)) continue;
      if (!preds.has(to)) preds.set(to, new Set());
      preds.get(to)!.add(from);
    }
  }

  // AllNodes for post-dom = reachable only (we exclude EXIT from materialized output)
  const allReachable = new Set(reachable);

  // Compute reverse postorder from EXIT in the reverse CFG
  // EXIT is virtual — get RPO over reachable starting from EXIT's neighbors
  const visited = new Set<string>();
  const postorder: string[] = [];

  function dfs(n: string): void {
    visited.add(n);
    const succs = revAdj.get(n);
    if (succs) {
      for (const s of succs) {
        if (reachable.has(s) && !visited.has(s)) {
          dfs(s);
        }
      }
    }
    if (n !== EXIT) postorder.push(n);
  }
  dfs(EXIT);
  const rpo = postorder.reverse();

  // Initialize PostDom
  const PostDom = new Map<string, Set<string>>();
  // For nodes reachable from EXIT in reverse CFG, initialize to {EXIT's neighbors} = proper start
  // EXIT dominates itself in reverse, but we exclude it
  PostDom.set(EXIT, new Set([EXIT]));
  for (const n of reachable) {
    if (revAdj.get(EXIT)?.has(n)) {
      // n is a direct successor of EXIT in reverse CFG (i.e., n exits to EXIT in forward)
      PostDom.set(n, new Set([n]));
    } else {
      PostDom.set(n, new Set(allReachable)); // init to all reachable
    }
  }

  // Iterate
  let changed = true;
  while (changed) {
    changed = false;
    for (const n of rpo) {
      const nodePreds = preds.get(n);
      if (!nodePreds) continue;

      // Filter preds to those we have PostDom entries for
      const validPreds = [...nodePreds].filter((p) => PostDom.has(p));
      if (validPreds.length === 0) continue;

      // Intersection of PostDom(p) for all valid preds p (in reverse CFG)
      let newPostDom = new Set<string>();
      let first = true;
      for (const p of validPreds) {
        const pdP = PostDom.get(p)!;
        if (first) {
          // Use reachable-only portion of pdP
          for (const d of pdP) {
            if (d !== EXIT) newPostDom.add(d);
          }
          first = false;
        } else {
          for (const d of newPostDom) {
            if (!pdP.has(d)) newPostDom.delete(d);
          }
        }
      }
      // Add n itself
      newPostDom.add(n);

      const oldPostDom = PostDom.get(n)!;
      if (newPostDom.size !== oldPostDom.size || [...newPostDom].some((d) => !oldPostDom.has(d))) {
        PostDom.set(n, newPostDom);
        changed = true;
      }
    }
  }

  return PostDom;
}

// ---------------------------------------------------------------------------
// Emit helpers
// ---------------------------------------------------------------------------

function emitDominance(tx: SastTx, Dom: Map<string, Set<string>>): void {
  const emitted = new Set<string>();

  for (const [n, domSet] of Dom) {
    if (n === EXIT) continue;
    for (const d of domSet) {
      if (d === EXIT) continue;
      const key = `${d}\0${n}`;
      if (emitted.has(key)) continue;
      emitted.add(key);
      tx.insert(dominance).values({ dominator: d, dominated: n }).run();
    }
  }
}

function emitPostDominance(tx: SastTx, PostDom: Map<string, Set<string>>): void {
  const emitted = new Set<string>();

  for (const [n, pdSet] of PostDom) {
    if (n === EXIT) continue;
    for (const pd of pdSet) {
      if (pd === EXIT) continue;
      const key = `${pd}\0${n}`;
      if (emitted.has(key)) continue;
      emitted.add(key);
      tx.insert(postDominance).values({ postDominator: pd, postDominated: n }).run();
    }
  }
}

// ---------------------------------------------------------------------------
// Per-function processing
// ---------------------------------------------------------------------------

function processFunction(
  tx: SastTx,
  fnNode: Node,
  nodeIdByNode: NodeIdMap,
): void {
  const cfgResult = buildCFG(fnNode, nodeIdByNode);
  if (!cfgResult) return;

  const { adj, entryId } = cfgResult;

  // Compute reachable nodes (DFS from entry, excluding EXIT)
  const reachable = reachableFrom(entryId, adj);
  if (reachable.size === 0) return;

  // Forward dominance
  const Dom = computeDominance(entryId, adj, reachable);
  emitDominance(tx, Dom);

  // Reverse CFG for post-dominance
  const { revAdj } = buildReverseCFG(adj, reachable);
  const PostDom = computePostDominance(revAdj, reachable);
  emitPostDominance(tx, PostDom);
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

export function extractDominance(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  // Walk all function-like nodes in the file
  sourceFile.forEachDescendant((node) => {
    if (FUNCTION_KINDS.has(node.getKind())) {
      processFunction(tx, node, nodeIdByNode);
    }
  });
}
