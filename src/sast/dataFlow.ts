/**
 * A4: Syntactic def-use data-flow edges.
 *
 * Pass 1: Walk every Identifier in the SourceFile, skip declarations and
 * property-access property names, resolve the DEF node via symbol API or
 * nodeBinding fallback (file-scoped), emit a data_flow row with the
 * appropriate slot from the closed vocabulary.
 *
 * Pass 1b (chain-formation, fixes the previous bipartite limitation):
 * for every VariableDeclaration with an initializer and every assignment
 * BinaryExpression (op === EqualsToken), emit one extra edge per Identifier
 * appearing in the RHS expression of shape `(decl_or_lhs_decl, rhs_use, "init")`.
 * That is: the declaration node ALSO becomes a to_node whose from_nodes are
 * the use-site identifiers feeding its initial/assigned value. This is what
 * makes chains form — the graph stops being bipartite.
 *
 * Pass 2: Compute transitive closure (DFS backward) and insert into
 * data_flow_transitive. O(N²) is fine for file-sized graphs.
 *
 * Why approach A (extra emit-time edges) over approach B (closure-time
 * bridging via node_binding name): the AST-walking pass already has full
 * parent context to identify decl/RHS pairs. Bridging in the closure pass
 * would mean adding name lookups against node_binding inside what is
 * currently a pure graph DFS. Cleaner to keep the closure dumb and put the
 * one extra edge emission in the place that already understands AST shape.
 *
 * Chain example for `function f(a){ const x=a; const y=x; return y; }`:
 *   direct edges include:
 *     (use_a, decl_a)                 — use_a is in init of x; reads param a
 *     (decl_x, use_a, "init")         — NEW: x's value flows from use_a
 *     (use_x, decl_x)                 — use_x in init of y reads decl_x
 *     (decl_y, use_x, "init")         — NEW
 *     (use_y_in_return, decl_y)
 *   transitive closure now contains (use_y_in_return, decl_a). No chain
 *   would form without the "init" edges.
 *
 * Interprocedural flow (e.g. `f(a){ g(a) } g(b){ use(b) }`) is NOT modeled.
 * Call-arg → callee-param edges would require name-resolved cross-function
 * binding, which is intentionally out of scope for the v1 syntactic substrate.
 */

import {
  SyntaxKind,
  type SourceFile,
  type Node,
  type Identifier,
  type BinaryExpression,
  type CallExpression,
} from "ts-morph";
import { eq, and } from "drizzle-orm";
import type { SastTx } from "./builder.js";
import { nodes as nodesTable, nodeBinding, dataFlow, dataFlowTransitive } from "./schema/index.js";
import type { NodeIdMap } from "./capabilities/extractor.js";

// ---------------------------------------------------------------------------
// Slot type (closed vocabulary)
// ---------------------------------------------------------------------------

type Slot =
  | "lhs"
  | "rhs"
  | "operand"
  | "denominator"
  | "condition"
  | "callee"
  | "arg[0]"
  | "arg[1]"
  | "arg[2]"
  | "arg[n]"
  | "return"
  | "iterable"
  | "element"
  | "property"
  | "index"
  | "throws"
  | "captures"
  | "read"
  // "init" is emitted on the SECOND edge in a def→use chain step:
  // for `const x = expr`, we emit (decl_x, ident_in_expr, "init") so that
  // the closure DFS chains through decl_x. Same for assignments `x = expr`.
  | "init";

const ARITHMETIC_OPS = new Set([
  SyntaxKind.PlusToken,
  SyntaxKind.MinusToken,
  SyntaxKind.AsteriskToken,
  SyntaxKind.SlashToken,
  SyntaxKind.PercentToken,
  SyntaxKind.AsteriskAsteriskToken,
  SyntaxKind.LessThanLessThanToken,
  SyntaxKind.GreaterThanGreaterThanToken,
  SyntaxKind.GreaterThanGreaterThanGreaterThanToken,
  SyntaxKind.AmpersandToken,
  SyntaxKind.BarToken,
  SyntaxKind.CaretToken,
]);

const ASSIGNMENT_OPS = new Set([
  SyntaxKind.EqualsToken,
  SyntaxKind.PlusEqualsToken,
  SyntaxKind.MinusEqualsToken,
  SyntaxKind.AsteriskEqualsToken,
  SyntaxKind.SlashEqualsToken,
  SyntaxKind.PercentEqualsToken,
  SyntaxKind.AsteriskAsteriskEqualsToken,
  SyntaxKind.LessThanLessThanEqualsToken,
  SyntaxKind.GreaterThanGreaterThanEqualsToken,
  SyntaxKind.AmpersandEqualsToken,
  SyntaxKind.BarEqualsToken,
  SyntaxKind.CaretEqualsToken,
  SyntaxKind.AmpersandAmpersandEqualsToken,
  SyntaxKind.BarBarEqualsToken,
  SyntaxKind.QuestionQuestionEqualsToken,
]);

// Kinds that declare a binding name (the identifier is the bound name, not a use)
const DECL_BINDING_PARENT_KINDS = new Set([
  SyntaxKind.VariableDeclaration,
  SyntaxKind.Parameter,
  SyntaxKind.FunctionDeclaration,
  SyntaxKind.ClassDeclaration,
  SyntaxKind.MethodDeclaration,
  SyntaxKind.FunctionExpression,
  SyntaxKind.BindingElement,
  SyntaxKind.PropertyDeclaration,
  SyntaxKind.GetAccessor,
  SyntaxKind.SetAccessor,
]);

/**
 * Returns true if this identifier is the binding-name child of a declaration
 * or is a property-access property name (not a reference).
 */
function isDeclarationName(ident: Identifier): boolean {
  const parent = ident.getParent();
  if (!parent) return false;
  const pk = parent.getKind();

  // Property name in a PropertyAccessExpression (foo.bar — `bar` is not a reference)
  if (pk === SyntaxKind.PropertyAccessExpression) {
    const pa = parent as import("ts-morph").PropertyAccessExpression;
    if (pa.getNameNode() === ident) return true;
  }

  // Qualified name (A.B — right side)
  if (pk === SyntaxKind.QualifiedName) {
    const qn = parent as import("ts-morph").QualifiedName;
    if (qn.getRight() === ident) return true;
  }

  // Import specifier name side
  if (pk === SyntaxKind.ImportSpecifier) {
    return true;
  }

  if (!DECL_BINDING_PARENT_KINDS.has(pk)) return false;

  // For declarations, only skip if this identifier IS the name node
  try {
    const nameNode = (parent as { getNameNode?: () => Node | undefined }).getNameNode?.();
    if (nameNode && nameNode === ident) return true;
  } catch {
    // Some nodes may not have getNameNode — fall through
  }

  return false;
}

// Function scope kinds
const SCOPE_KINDS = new Set([
  SyntaxKind.FunctionDeclaration,
  SyntaxKind.FunctionExpression,
  SyntaxKind.ArrowFunction,
  SyntaxKind.MethodDeclaration,
  SyntaxKind.SourceFile,
]);

/**
 * Determine the slot for an identifier given its enclosing expression context.
 * Walk ancestors upward, take the INNERMOST matching context.
 */
function resolveSlot(ident: Identifier): Slot {
  let node: Node = ident;
  let parent = node.getParent();

  while (parent) {
    const pk = parent.getKind();

    // BinaryExpression
    if (pk === SyntaxKind.BinaryExpression) {
      const bin = parent as BinaryExpression;
      const opKind = bin.getOperatorToken().getKind();
      const left = bin.getLeft();
      const right = bin.getRight();

      // For immediate children only: check if node is directly left or right
      const directlyLeft = node === left;
      const directlyRight = node === right;

      if (directlyLeft || directlyRight) {
        if (ARITHMETIC_OPS.has(opKind)) {
          if (directlyLeft) return "lhs";
          if (directlyRight && opKind === SyntaxKind.SlashToken) return "denominator";
          if (directlyRight) return "rhs";
        }
        if (ASSIGNMENT_OPS.has(opKind)) {
          if (directlyLeft) return "lhs";
          if (directlyRight) return "rhs";
        }
        // comparison, logical — treat as rhs/lhs or read
        if (directlyLeft) return "lhs";
        if (directlyRight) return "rhs";
      }
      // ident is nested inside left or right — keep going up
    }

    // PrefixUnaryExpression / PostfixUnaryExpression
    if (
      pk === SyntaxKind.PrefixUnaryExpression ||
      pk === SyntaxKind.PostfixUnaryExpression
    ) {
      return "operand";
    }

    // Condition slots
    if (pk === SyntaxKind.IfStatement) {
      const ifStmt = parent as import("ts-morph").IfStatement;
      if (node === ifStmt.getExpression()) return "condition";
    }
    if (pk === SyntaxKind.ConditionalExpression) {
      const cond = parent as import("ts-morph").ConditionalExpression;
      if (node === cond.getCondition()) return "condition";
    }
    if (pk === SyntaxKind.WhileStatement) {
      const ws = parent as import("ts-morph").WhileStatement;
      if (node === ws.getExpression()) return "condition";
    }
    if (pk === SyntaxKind.ForStatement) {
      const fs = parent as import("ts-morph").ForStatement;
      if (node === fs.getCondition()) return "condition";
    }

    // CallExpression
    if (pk === SyntaxKind.CallExpression) {
      const call = parent as CallExpression;
      const calleeExpr = call.getExpression();
      const args = call.getArguments();

      // Is ident inside the callee?
      if (
        node === calleeExpr ||
        calleeExpr.getDescendantsOfKind(SyntaxKind.Identifier).some((id) => id === ident)
      ) {
        return "callee";
      }

      // Is ident an argument?
      for (let i = 0; i < args.length; i++) {
        const arg = args[i];
        if (
          node === arg ||
          arg.getDescendantsOfKind(SyntaxKind.Identifier).some((id) => id === ident)
        ) {
          if (i === 0) return "arg[0]";
          if (i === 1) return "arg[1]";
          if (i === 2) return "arg[2]";
          return "arg[n]";
        }
      }
    }

    // ReturnStatement
    if (pk === SyntaxKind.ReturnStatement) {
      const ret = parent as import("ts-morph").ReturnStatement;
      if (node === ret.getExpression()) return "return";
    }

    // ForOfStatement / ForInStatement iterable
    if (pk === SyntaxKind.ForOfStatement) {
      const fof = parent as import("ts-morph").ForOfStatement;
      if (node === fof.getExpression()) return "iterable";
    }
    if (pk === SyntaxKind.ForInStatement) {
      const fin = parent as import("ts-morph").ForInStatement;
      if (node === fin.getExpression()) return "iterable";
    }

    // ElementAccessExpression
    if (pk === SyntaxKind.ElementAccessExpression) {
      const ea = parent as import("ts-morph").ElementAccessExpression;
      if (node === ea.getExpression()) return "element";
      if (node === ea.getArgumentExpression()) return "index";
    }

    // PropertyAccessExpression — object side
    if (pk === SyntaxKind.PropertyAccessExpression) {
      const pa = parent as import("ts-morph").PropertyAccessExpression;
      if (node === pa.getExpression()) return "read";
      // property name side is filtered in isDeclarationName — shouldn't reach here
    }

    // ThrowStatement
    if (pk === SyntaxKind.ThrowStatement) {
      return "throws";
    }

    // VariableDeclaration initializer (const x = <ident>)
    if (pk === SyntaxKind.VariableDeclaration) {
      const vd = parent as import("ts-morph").VariableDeclaration;
      if (node === vd.getInitializer()) return "rhs";
    }

    // Stop at function boundary — don't leak across scope
    if (SCOPE_KINDS.has(pk)) break;

    node = parent;
    parent = parent.getParent();
  }

  return "read";
}

/**
 * Walk ancestors to find the enclosing function kind (if any).
 * Used to detect cross-scope (captures) usage.
 */
function enclosingFunctionStart(ident: Identifier): number | undefined {
  let parent: Node | undefined = ident.getParent();
  while (parent) {
    const pk = parent.getKind();
    if (
      pk === SyntaxKind.FunctionDeclaration ||
      pk === SyntaxKind.FunctionExpression ||
      pk === SyntaxKind.ArrowFunction ||
      pk === SyntaxKind.MethodDeclaration
    ) {
      return parent.getStart();
    }
    parent = parent.getParent();
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// Pass 1 — emit direct edges
// ---------------------------------------------------------------------------

function emitDirectEdges(
  tx: SastTx,
  fileId: number,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  // Set to dedupe (to_node, from_node, slot) — avoid duplicate key errors
  const emitted = new Set<string>();

  function emit(toNodeId: string, fromNodeId: string, slot: Slot): void {
    const key = `${toNodeId}\0${fromNodeId}\0${slot}`;
    if (emitted.has(key)) return;
    emitted.add(key);
    tx.insert(dataFlow).values({ toNode: toNodeId, fromNode: fromNodeId, slot }).run();
  }

  sourceFile.forEachDescendant((node) => {
    if (node.getKind() !== SyntaxKind.Identifier) return;
    const ident = node as Identifier;

    // Skip if this is a declaration name or property-access property name
    if (isDeclarationName(ident)) return;

    const useId = nodeIdByNode.get(ident);
    if (!useId) return;

    // Resolve declaration node
    let declNode: Node | undefined;
    let declStart: number | undefined;

    // Try symbol API first
    try {
      const sym = ident.getSymbol();
      if (sym) {
        const decls = sym.getDeclarations();
        if (decls && decls.length > 0) {
          const first = decls[0];
          // Only use if same file
          if (first.getSourceFile() === sourceFile) {
            declNode = first;
            declStart = first.getStart();
          }
        }
      }
    } catch {
      // Symbol resolution failed — try fallback
    }

    // Fallback: query node_binding for same name in same file
    if (!declNode) {
      const name = ident.getText();
      const bindings = tx
        .select({ nodeId: nodeBinding.nodeId })
        .from(nodeBinding)
        .innerJoin(nodesTable, eq(nodeBinding.nodeId, nodesTable.id))
        .where(and(eq(nodeBinding.name, name), eq(nodesTable.fileId, fileId)))
        .all();

      if (bindings.length > 0) {
        // Pick the binding whose node is most recently declared before the use
        const useStart = ident.getStart();
        let bestId: string | undefined;
        let bestStart = -1;

        for (const b of bindings) {
          // Get node start from nodes table
          const nRow = tx
            .select({ sourceStart: nodesTable.sourceStart })
            .from(nodesTable)
            .where(eq(nodesTable.id, b.nodeId))
            .get();
          if (nRow && nRow.sourceStart <= useStart && nRow.sourceStart > bestStart) {
            bestStart = nRow.sourceStart;
            bestId = b.nodeId;
            declStart = nRow.sourceStart;
          }
        }

        if (bestId) {
          // Emit edge using raw node ID
          const slot = resolveSlot(ident);
          emit(useId, bestId, slot);

          // Captures: if use is inside a function and decl is outside it
          const fnStart = enclosingFunctionStart(ident);
          if (fnStart !== undefined && declStart !== undefined && declStart < fnStart) {
            emit(useId, bestId, "captures");
          }
          return;
        }
      }
      return; // Nothing found — skip
    }

    const declId = nodeIdByNode.get(declNode);
    if (!declId) return;

    const slot = resolveSlot(ident);
    emit(useId, declId, slot);

    // Captures: cross-scope
    if (declStart !== undefined) {
      const fnStart = enclosingFunctionStart(ident);
      if (fnStart !== undefined && declStart < fnStart) {
        emit(useId, declId, "captures");
      }
    }
  });

  // -------------------------------------------------------------------------
  // Pass 1b: chain-formation edges. For every VariableDeclaration with an
  // initializer, and every assignment BinaryExpression (op === EqualsToken or
  // a compound-assignment), emit (lhs_decl, rhs_use_ident, "init") for every
  // identifier appearing in the initializer/RHS expression.
  //
  // This is what fixes the bipartite-graph limitation: declaration nodes
  // become to_nodes (predecessors of their RHS uses), so the closure DFS can
  // chain from a use of a chained variable back to the original source.
  // -------------------------------------------------------------------------

  function emitInitChainFromExpr(targetDeclId: string, expr: Node): void {
    // Walk identifiers in the expression. Skip declaration names + property
    // access right-hand identifiers (same predicate used above).
    expr.forEachDescendant((d) => {
      if (d.getKind() !== SyntaxKind.Identifier) return;
      const id = d as Identifier;
      if (isDeclarationName(id)) return;
      const useId = nodeIdByNode.get(id);
      if (!useId) return;
      // Don't self-loop: targetDecl is its own LHS name; the initializer
      // identifiers won't be the decl name node itself, but defend anyway.
      if (useId === targetDeclId) return;
      emit(targetDeclId, useId, "init");
    });
    // Edge case: the initializer IS itself a single Identifier (forEachDescendant
    // does not visit the root). Cover that.
    if (expr.getKind() === SyntaxKind.Identifier) {
      const id = expr as Identifier;
      if (!isDeclarationName(id)) {
        const useId = nodeIdByNode.get(id);
        if (useId && useId !== targetDeclId) {
          emit(targetDeclId, useId, "init");
        }
      }
    }
  }

  function resolveAssignmentTargetDeclId(lhs: Node): string | undefined {
    // Only handle simple identifier LHS for now; property/element access
    // assignments (`o.x = …`, `arr[i] = …`) don't have a single decl target
    // we can attribute the chain to.
    if (lhs.getKind() !== SyntaxKind.Identifier) return undefined;
    const ident = lhs as Identifier;

    // Try symbol API
    try {
      const sym = ident.getSymbol();
      if (sym) {
        const decls = sym.getDeclarations();
        if (decls && decls.length > 0) {
          const first = decls[0];
          if (first.getSourceFile() === sourceFile) {
            return nodeIdByNode.get(first);
          }
        }
      }
    } catch {
      // fall through to nodeBinding
    }

    // Fallback: nodeBinding (most recent binding before this LHS in the file)
    const name = ident.getText();
    const bindings = tx
      .select({ nodeId: nodeBinding.nodeId })
      .from(nodeBinding)
      .innerJoin(nodesTable, eq(nodeBinding.nodeId, nodesTable.id))
      .where(and(eq(nodeBinding.name, name), eq(nodesTable.fileId, fileId)))
      .all();
    if (bindings.length === 0) return undefined;

    const useStart = ident.getStart();
    let bestId: string | undefined;
    let bestStart = -1;
    for (const b of bindings) {
      const nRow = tx
        .select({ sourceStart: nodesTable.sourceStart })
        .from(nodesTable)
        .where(eq(nodesTable.id, b.nodeId))
        .get();
      if (nRow && nRow.sourceStart <= useStart && nRow.sourceStart > bestStart) {
        bestStart = nRow.sourceStart;
        bestId = b.nodeId;
      }
    }
    return bestId;
  }

  sourceFile.forEachDescendant((node) => {
    const k = node.getKind();

    // VariableDeclaration: const/let/var x = expr;
    if (k === SyntaxKind.VariableDeclaration) {
      const vd = node as import("ts-morph").VariableDeclaration;
      const init = vd.getInitializer();
      if (!init) return;
      const declId = nodeIdByNode.get(vd);
      if (!declId) return;
      emitInitChainFromExpr(declId, init);
      return;
    }

    // BinaryExpression with assignment op: x = expr; x += expr; ...
    if (k === SyntaxKind.BinaryExpression) {
      const bin = node as BinaryExpression;
      const opKind = bin.getOperatorToken().getKind();
      if (!ASSIGNMENT_OPS.has(opKind)) return;
      const targetDeclId = resolveAssignmentTargetDeclId(bin.getLeft());
      if (!targetDeclId) return;
      emitInitChainFromExpr(targetDeclId, bin.getRight());
      return;
    }

    // Parameter with default: function f(x = expr) { ... }
    if (k === SyntaxKind.Parameter) {
      const param = node as import("ts-morph").ParameterDeclaration;
      const init = param.getInitializer();
      if (!init) return;
      const declId = nodeIdByNode.get(param);
      if (!declId) return;
      emitInitChainFromExpr(declId, init);
      return;
    }
  });
}

// ---------------------------------------------------------------------------
// Pass 2 — transitive closure
// ---------------------------------------------------------------------------

function emitTransitiveClosure(tx: SastTx, fileId: number): void {
  // Load direct edges scoped to this file (no cross-file edges are emitted,
  // so filtering on toNode's fileId is sufficient). Multi-file builds would
  // otherwise re-process edges from prior files and hit PK conflicts.
  const direct = tx
    .select({ toNode: dataFlow.toNode, fromNode: dataFlow.fromNode })
    .from(dataFlow)
    .innerJoin(nodesTable, eq(nodesTable.id, dataFlow.toNode))
    .where(eq(nodesTable.fileId, fileId))
    .all();

  // Build adjacency: for each to_node, what are its from_nodes?
  const predecessors = new Map<string, Set<string>>();
  for (const { toNode, fromNode } of direct) {
    if (!predecessors.has(toNode)) predecessors.set(toNode, new Set());
    predecessors.get(toNode)!.add(fromNode);
  }

  const emitted = new Set<string>();

  function emit(toNode: string, fromNode: string): void {
    const key = `${toNode}\0${fromNode}`;
    if (emitted.has(key)) return;
    emitted.add(key);
    tx.insert(dataFlowTransitive).values({ toNode, fromNode }).run();
  }

  // DFS backward from each node: collect all reachable ancestors
  const cache = new Map<string, Set<string>>();

  function ancestors(node: string): Set<string> {
    if (cache.has(node)) return cache.get(node)!;
    const result = new Set<string>();
    cache.set(node, result); // break cycles
    const preds = predecessors.get(node);
    if (preds) {
      for (const pred of preds) {
        result.add(pred);
        for (const anc of ancestors(pred)) {
          result.add(anc);
        }
      }
    }
    return result;
  }

  // Compute for all nodes that appear as to_node in direct edges
  const allToNodes = new Set(direct.map((e) => e.toNode));
  for (const toNode of allToNodes) {
    for (const fromNode of ancestors(toNode)) {
      emit(toNode, fromNode);
    }
  }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

export function extractDataFlow(
  tx: SastTx,
  fileId: number,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  emitDirectEdges(tx, fileId, sourceFile, nodeIdByNode);
  emitTransitiveClosure(tx, fileId);
}
