/**
 * A4: Syntactic def-use data-flow edges.
 *
 * Pass 1: Walk every Identifier in the SourceFile, skip declarations and
 * property-access property names, resolve the DEF node via symbol API or
 * nodeBinding fallback (file-scoped), emit a data_flow row with the
 * appropriate slot from the closed vocabulary.
 *
 * Pass 2: Compute transitive closure (DFS backward) and insert into
 * data_flow_transitive. O(N²) is fine for file-sized graphs.
 *
 * KNOWN LIMITATION (surfaced during A4 review): the current edge shape
 * produces a bipartite graph — declarations are only from_nodes, use-site
 * identifiers are only to_nodes. Transitive closure therefore equals direct
 * edges; no chains form. If A7 DSL or C4 complementary-site discovery needs
 * chained reachability ("param a ultimately flows into y/b via x and y"),
 * the edge shape needs redesign — either (a) emit def→binding→use triples
 * so chains form, or (b) populate data_flow_transitive by joining through
 * node_binding name. Revisit when a consumer needs it.
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
  | "read";

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
      const isLeft = node === left || left.getDescendantsOfKind(SyntaxKind.Identifier).includes(ident as Identifier) && !right.getDescendantsOfKind(SyntaxKind.Identifier).includes(ident as Identifier);

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
}

// ---------------------------------------------------------------------------
// Pass 2 — transitive closure
// ---------------------------------------------------------------------------

function emitTransitiveClosure(tx: SastTx): void {
  // Load all direct edges
  const direct = tx.select({ toNode: dataFlow.toNode, fromNode: dataFlow.fromNode }).from(dataFlow).all();

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
  emitTransitiveClosure(tx);
}
