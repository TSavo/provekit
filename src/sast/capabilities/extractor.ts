/**
 * A3b: 16 capability extractors.
 *
 * Each extractor walks the SourceFile using forEachDescendant / getDescendantsOfKind
 * and inserts rows into the corresponding capability table.
 *
 * `nodeIdByNode` maps ts-morph Node references (stable within one Project) to
 * the string IDs assigned during walkIterative.  If a lookup returns undefined
 * we skip that row rather than crashing — getChildren() and forEachDescendant()
 * visit slightly different node sets, so missing entries are possible on edge nodes.
 */

import {
  SyntaxKind,
  type Node,
  type SourceFile,
  type BinaryExpression,
  type PropertyAccessExpression,
  type ElementAccessExpression,
  type IfStatement,
  type ConditionalExpression,
  type SwitchStatement,
  type ForStatement,
  type WhileStatement,
  type DoStatement,
  type ForOfStatement,
  type ForInStatement,
  type CallExpression,
  type FunctionDeclaration,
  type FunctionExpression,
  type ArrowFunction,
  type VariableDeclaration,
  type ParameterDeclaration,
  type ClassDeclaration,
  type BindingElement,
  type TemplateExpression,
} from "ts-morph";
import type { SastTx } from "../builder.js";
import {
  nodeArithmetic,
  nodeAssigns,
  nodeReturns,
  nodeMemberAccess,
  nodeNonNullAssertion,
  nodeTruthiness,
  nodeNarrows,
  nodeDecides,
  nodeIterates,
  nodeYields,
  nodeThrows,
  nodeCalls,
  nodeCaptures,
  nodePattern,
  nodeBinding,
  nodeSignal,
  signalInterpolations,
} from "../schema/capabilities/index.js";

export type NodeIdMap = Map<Node, string>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Safely get ID from map; return undefined if node itself is undefined/null */
function id(nodeIdByNode: NodeIdMap, node: Node | undefined | null): string | undefined {
  if (!node) return undefined;
  return nodeIdByNode.get(node);
}

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

// ---------------------------------------------------------------------------
// 1. extractArithmetic
// ---------------------------------------------------------------------------

export function extractArithmetic(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() !== SyntaxKind.BinaryExpression) return;
    const bin = node as BinaryExpression;
    const opKind = bin.getOperatorToken().getKind();
    if (!ARITHMETIC_OPS.has(opKind)) return;

    const nodeId = nodeIdByNode.get(bin);
    const lhsId = id(nodeIdByNode, bin.getLeft());
    const rhsId = id(nodeIdByNode, bin.getRight());
    if (!nodeId || !lhsId || !rhsId) return;

    tx.insert(nodeArithmetic).values({
      nodeId,
      op: bin.getOperatorToken().getText(),
      lhsNode: lhsId,
      rhsNode: rhsId,
      resultSort: "Real",
    }).run();
  });
}

// ---------------------------------------------------------------------------
// 2. extractAssigns
// ---------------------------------------------------------------------------

export function extractAssigns(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.DeleteExpression) {
      const nodeId = nodeIdByNode.get(node);
      const operand = (node as import("ts-morph").DeleteExpression).getExpression();
      const targetId = id(nodeIdByNode, operand);
      if (!nodeId || !targetId) return;
      tx.insert(nodeAssigns).values({
        nodeId,
        targetNode: targetId,
        rhsNode: null,
        assignKind: "delete",
      }).run();
      return;
    }

    if (node.getKind() !== SyntaxKind.BinaryExpression) return;
    const bin = node as BinaryExpression;
    const opKind = bin.getOperatorToken().getKind();
    if (!ASSIGNMENT_OPS.has(opKind)) return;

    const nodeId = nodeIdByNode.get(bin);
    const targetId = id(nodeIdByNode, bin.getLeft());
    const rhsId = id(nodeIdByNode, bin.getRight());
    if (!nodeId || !targetId) return;

    tx.insert(nodeAssigns).values({
      nodeId,
      targetNode: targetId,
      rhsNode: rhsId ?? null,
      assignKind: bin.getOperatorToken().getText(),
    }).run();
  });
}

// ---------------------------------------------------------------------------
// 3. extractReturns
// ---------------------------------------------------------------------------

export function extractReturns(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.ReturnStatement) {
      const ret = node as import("ts-morph").ReturnStatement;
      const nodeId = nodeIdByNode.get(node);
      if (!nodeId) return;
      const expr = ret.getExpression();
      tx.insert(nodeReturns).values({
        nodeId,
        exitKind: "return",
        valueNode: expr ? (id(nodeIdByNode, expr) ?? null) : null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.ThrowStatement) {
      const thr = node as import("ts-morph").ThrowStatement;
      const nodeId = nodeIdByNode.get(node);
      if (!nodeId) return;
      const expr = thr.getExpression();
      tx.insert(nodeReturns).values({
        nodeId,
        exitKind: "throw",
        valueNode: expr ? (id(nodeIdByNode, expr) ?? null) : null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.CallExpression) {
      const call = node as CallExpression;
      const expr = call.getExpression();
      if (expr.getText() === "process.exit") {
        const nodeId = nodeIdByNode.get(node);
        if (!nodeId) return;
        const args = call.getArguments();
        tx.insert(nodeReturns).values({
          nodeId,
          exitKind: "process_exit",
          valueNode: args[0] ? (id(nodeIdByNode, args[0]) ?? null) : null,
        }).run();
      }
    }
  });
}

// ---------------------------------------------------------------------------
// 4. extractMemberAccess
// ---------------------------------------------------------------------------

export function extractMemberAccess(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.PropertyAccessExpression) {
      const pa = node as PropertyAccessExpression;
      const nodeId = nodeIdByNode.get(pa);
      const objectId = id(nodeIdByNode, pa.getExpression());
      if (!nodeId || !objectId) return;
      tx.insert(nodeMemberAccess).values({
        nodeId,
        objectNode: objectId,
        propertyName: pa.getName(),
        computed: false,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.ElementAccessExpression) {
      const ea = node as ElementAccessExpression;
      const nodeId = nodeIdByNode.get(ea);
      const objectId = id(nodeIdByNode, ea.getExpression());
      if (!nodeId || !objectId) return;
      tx.insert(nodeMemberAccess).values({
        nodeId,
        objectNode: objectId,
        propertyName: null,
        computed: true,
      }).run();
    }
  });
}

// ---------------------------------------------------------------------------
// 5. extractNonNullAssertion
// ---------------------------------------------------------------------------

export function extractNonNullAssertion(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() !== SyntaxKind.NonNullExpression) return;
    const nodeId = nodeIdByNode.get(node);
    const operandId = id(nodeIdByNode, node.getChildren()[0]);
    if (!nodeId || !operandId) return;
    tx.insert(nodeNonNullAssertion).values({
      nodeId,
      operandNode: operandId,
    }).run();
  });
}

// ---------------------------------------------------------------------------
// 6. extractTruthiness
// ---------------------------------------------------------------------------

export function extractTruthiness(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.BinaryExpression) {
      const bin = node as BinaryExpression;
      const opKind = bin.getOperatorToken().getKind();

      if (opKind === SyntaxKind.BarBarToken) {
        const nodeId = nodeIdByNode.get(bin);
        const operandId = id(nodeIdByNode, bin.getLeft());
        if (!nodeId || !operandId) return;
        tx.insert(nodeTruthiness).values({
          nodeId,
          coercionKind: "falsy_default",
          operandNode: operandId,
        }).run();
        return;
      }

      if (opKind === SyntaxKind.QuestionQuestionToken) {
        const nodeId = nodeIdByNode.get(bin);
        const operandId = id(nodeIdByNode, bin.getLeft());
        if (!nodeId || !operandId) return;
        tx.insert(nodeTruthiness).values({
          nodeId,
          coercionKind: "nullish_coalesce",
          operandNode: operandId,
        }).run();
        return;
      }

      if (opKind === SyntaxKind.EqualsEqualsEqualsToken) {
        // a === null
        const left = bin.getLeft();
        const right = bin.getRight();
        let nonNullSide: Node | undefined;
        if (right.getKind() === SyntaxKind.NullKeyword) {
          nonNullSide = left;
        } else if (left.getKind() === SyntaxKind.NullKeyword) {
          nonNullSide = right;
        }
        if (nonNullSide) {
          const nodeId = nodeIdByNode.get(bin);
          const operandId = id(nodeIdByNode, nonNullSide);
          if (!nodeId || !operandId) return;
          tx.insert(nodeTruthiness).values({
            nodeId,
            coercionKind: "strict_eq_null",
            operandNode: operandId,
          }).run();
        }
        return;
      }

      return;
    }

    // IfStatement: condition that is not a BinaryExpression
    if (node.getKind() === SyntaxKind.IfStatement) {
      const ifStmt = node as IfStatement;
      const condition = ifStmt.getExpression();
      if (condition.getKind() !== SyntaxKind.BinaryExpression) {
        const nodeId = nodeIdByNode.get(ifStmt);
        const operandId = id(nodeIdByNode, condition);
        if (!nodeId || !operandId) return;
        tx.insert(nodeTruthiness).values({
          nodeId,
          coercionKind: "truthy_test",
          operandNode: operandId,
        }).run();
      }
      return;
    }

    // ConditionalExpression: condition that is not a BinaryExpression
    if (node.getKind() === SyntaxKind.ConditionalExpression) {
      const cond = node as ConditionalExpression;
      const condition = cond.getCondition();
      if (condition.getKind() !== SyntaxKind.BinaryExpression) {
        const nodeId = nodeIdByNode.get(cond);
        const operandId = id(nodeIdByNode, condition);
        if (!nodeId || !operandId) return;
        tx.insert(nodeTruthiness).values({
          nodeId,
          coercionKind: "truthy_test",
          operandNode: operandId,
        }).run();
      }
    }
  });
}

// ---------------------------------------------------------------------------
// 7. extractNarrows
// ---------------------------------------------------------------------------

export function extractNarrows(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  const EQ_OPS = new Set([
    SyntaxKind.EqualsEqualsEqualsToken,
    SyntaxKind.ExclamationEqualsEqualsToken,
    SyntaxKind.EqualsEqualsToken,
    SyntaxKind.ExclamationEqualsToken,
  ]);

  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.BinaryExpression) {
      const bin = node as BinaryExpression;
      const opKind = bin.getOperatorToken().getKind();
      const left = bin.getLeft();
      const right = bin.getRight();

      if (opKind === SyntaxKind.InstanceOfKeyword) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, left);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "instanceof",
          narrowedType: right.getText(),
        }).run();
        return;
      }

      if (opKind === SyntaxKind.InKeyword) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, right);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "in",
          narrowedType: left.getText(),
        }).run();
        return;
      }

      if (!EQ_OPS.has(opKind)) return;

      // Check for typeof on either side
      if (left.getKind() === SyntaxKind.TypeOfExpression) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, (left as import("ts-morph").TypeOfExpression).getExpression());
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "typeof",
          narrowedType: right.getText().replace(/['"]/g, ""),
        }).run();
        return;
      }
      if (right.getKind() === SyntaxKind.TypeOfExpression) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, (right as import("ts-morph").TypeOfExpression).getExpression());
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "typeof",
          narrowedType: left.getText().replace(/['"]/g, ""),
        }).run();
        return;
      }

      // null check
      if (right.getKind() === SyntaxKind.NullKeyword) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, left);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "null_check",
          narrowedType: null,
        }).run();
        return;
      }
      if (left.getKind() === SyntaxKind.NullKeyword) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, right);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "null_check",
          narrowedType: null,
        }).run();
        return;
      }

      // undefined check
      if (right.getKind() === SyntaxKind.Identifier && right.getText() === "undefined") {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, left);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "undefined_check",
          narrowedType: null,
        }).run();
        return;
      }
      if (left.getKind() === SyntaxKind.Identifier && left.getText() === "undefined") {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, right);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "undefined_check",
          narrowedType: null,
        }).run();
        return;
      }

      // literal_eq: one side is a string/number/true/false literal
      const LITERAL_KINDS = new Set([
        SyntaxKind.StringLiteral,
        SyntaxKind.NumericLiteral,
        SyntaxKind.TrueKeyword,
        SyntaxKind.FalseKeyword,
      ]);
      if (LITERAL_KINDS.has(right.getKind())) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, left);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "literal_eq",
          narrowedType: right.getText(),
        }).run();
        return;
      }
      if (LITERAL_KINDS.has(left.getKind())) {
        const nodeId = nodeIdByNode.get(bin);
        const targetId = id(nodeIdByNode, right);
        if (!nodeId || !targetId) return;
        tx.insert(nodeNarrows).values({
          nodeId,
          targetNode: targetId,
          narrowingKind: "literal_eq",
          narrowedType: left.getText(),
        }).run();
        return;
      }
      return;
    }
  });
}

// ---------------------------------------------------------------------------
// 8. extractDecides
// ---------------------------------------------------------------------------

export function extractDecides(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.IfStatement) {
      const ifStmt = node as IfStatement;
      const nodeId = nodeIdByNode.get(ifStmt);
      const condId = id(nodeIdByNode, ifStmt.getExpression());
      if (!nodeId || !condId) return;
      const consequent = ifStmt.getThenStatement();
      const alternate = ifStmt.getElseStatement();
      tx.insert(nodeDecides).values({
        nodeId,
        conditionNode: condId,
        consequentNode: id(nodeIdByNode, consequent) ?? null,
        alternateNode: alternate ? (id(nodeIdByNode, alternate) ?? null) : null,
        decisionKind: "if",
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.ConditionalExpression) {
      const cond = node as ConditionalExpression;
      const nodeId = nodeIdByNode.get(cond);
      const condId = id(nodeIdByNode, cond.getCondition());
      if (!nodeId || !condId) return;
      tx.insert(nodeDecides).values({
        nodeId,
        conditionNode: condId,
        consequentNode: id(nodeIdByNode, cond.getWhenTrue()) ?? null,
        alternateNode: id(nodeIdByNode, cond.getWhenFalse()) ?? null,
        decisionKind: "ternary",
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.SwitchStatement) {
      const sw = node as SwitchStatement;
      const nodeId = nodeIdByNode.get(sw);
      const condId = id(nodeIdByNode, sw.getExpression());
      if (!nodeId || !condId) return;
      tx.insert(nodeDecides).values({
        nodeId,
        conditionNode: condId,
        consequentNode: null,
        alternateNode: null,
        decisionKind: "switch",
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.BinaryExpression) {
      const bin = node as BinaryExpression;
      const opKind = bin.getOperatorToken().getKind();
      let decisionKind: string | null = null;
      if (opKind === SyntaxKind.AmpersandAmpersandToken) decisionKind = "short_circuit_and";
      else if (opKind === SyntaxKind.BarBarToken) decisionKind = "short_circuit_or";
      else if (opKind === SyntaxKind.QuestionQuestionToken) decisionKind = "nullish";
      if (!decisionKind) return;

      const nodeId = nodeIdByNode.get(bin);
      const condId = id(nodeIdByNode, bin.getLeft());
      if (!nodeId || !condId) return;
      tx.insert(nodeDecides).values({
        nodeId,
        conditionNode: condId,
        consequentNode: id(nodeIdByNode, bin.getRight()) ?? null,
        alternateNode: null,
        decisionKind,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.CallExpression) {
      const call = node as CallExpression;
      // optional chain: foo?.bar()
      if (call.hasQuestionDotToken()) {
        const nodeId = nodeIdByNode.get(call);
        const condId = id(nodeIdByNode, call.getExpression());
        if (!nodeId || !condId) return;
        tx.insert(nodeDecides).values({
          nodeId,
          conditionNode: condId,
          consequentNode: null,
          alternateNode: null,
          decisionKind: "optional_chain",
        }).run();
      }
    }
  });
}

// ---------------------------------------------------------------------------
// 9. extractIterates
// ---------------------------------------------------------------------------

export function extractIterates(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.ForStatement) {
      const forStmt = node as ForStatement;
      const nodeId = nodeIdByNode.get(forStmt);
      const bodyId = id(nodeIdByNode, forStmt.getStatement());
      if (!nodeId || !bodyId) return;
      tx.insert(nodeIterates).values({
        nodeId,
        initNode: id(nodeIdByNode, forStmt.getInitializer()) ?? null,
        conditionNode: id(nodeIdByNode, forStmt.getCondition()) ?? null,
        updateNode: id(nodeIdByNode, forStmt.getIncrementor()) ?? null,
        bodyNode: bodyId,
        loopKind: "for",
        executesAtLeastOnce: false,
        collectionSourceNode: null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.WhileStatement) {
      const w = node as WhileStatement;
      const nodeId = nodeIdByNode.get(w);
      const condId = id(nodeIdByNode, w.getExpression());
      const bodyId = id(nodeIdByNode, w.getStatement());
      if (!nodeId || !condId || !bodyId) return;
      tx.insert(nodeIterates).values({
        nodeId,
        initNode: null,
        conditionNode: condId,
        updateNode: null,
        bodyNode: bodyId,
        loopKind: "while",
        executesAtLeastOnce: false,
        collectionSourceNode: null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.DoStatement) {
      const d = node as DoStatement;
      const nodeId = nodeIdByNode.get(d);
      const condId = id(nodeIdByNode, d.getExpression());
      const bodyId = id(nodeIdByNode, d.getStatement());
      if (!nodeId || !condId || !bodyId) return;
      tx.insert(nodeIterates).values({
        nodeId,
        initNode: null,
        conditionNode: condId,
        updateNode: null,
        bodyNode: bodyId,
        loopKind: "do_while",
        executesAtLeastOnce: true,
        collectionSourceNode: null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.ForOfStatement) {
      const fof = node as ForOfStatement;
      const nodeId = nodeIdByNode.get(fof);
      const bodyId = id(nodeIdByNode, fof.getStatement());
      const collId = id(nodeIdByNode, fof.getExpression());
      if (!nodeId || !bodyId) return;
      tx.insert(nodeIterates).values({
        nodeId,
        initNode: id(nodeIdByNode, fof.getInitializer()) ?? null,
        conditionNode: null,
        updateNode: null,
        bodyNode: bodyId,
        loopKind: "for_of",
        executesAtLeastOnce: false,
        collectionSourceNode: collId ?? null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.ForInStatement) {
      const fin = node as ForInStatement;
      const nodeId = nodeIdByNode.get(fin);
      const bodyId = id(nodeIdByNode, fin.getStatement());
      const collId = id(nodeIdByNode, fin.getExpression());
      if (!nodeId || !bodyId) return;
      tx.insert(nodeIterates).values({
        nodeId,
        initNode: id(nodeIdByNode, fin.getInitializer()) ?? null,
        conditionNode: null,
        updateNode: null,
        bodyNode: bodyId,
        loopKind: "for_in",
        executesAtLeastOnce: false,
        collectionSourceNode: collId ?? null,
      }).run();
    }
  });
}

// ---------------------------------------------------------------------------
// 10. extractYields
// ---------------------------------------------------------------------------

export function extractYields(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.AwaitExpression) {
      const nodeId = nodeIdByNode.get(node);
      if (!nodeId) return;
      const expr = node.getChildren().find(
        (c) => c.getKind() !== SyntaxKind.AwaitKeyword,
      );
      const sourceCallId =
        expr && expr.getKind() === SyntaxKind.CallExpression
          ? id(nodeIdByNode, expr)
          : undefined;
      tx.insert(nodeYields).values({
        nodeId,
        yieldKind: "await",
        sourceCallNode: sourceCallId ?? null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.YieldExpression) {
      const nodeId = nodeIdByNode.get(node);
      if (!nodeId) return;
      const yieldExpr = node as import("ts-morph").YieldExpression;
      const expr = yieldExpr.getExpression();
      const sourceCallId =
        expr && expr.getKind() === SyntaxKind.CallExpression
          ? id(nodeIdByNode, expr)
          : undefined;
      tx.insert(nodeYields).values({
        nodeId,
        yieldKind: "yield",
        sourceCallNode: sourceCallId ?? null,
      }).run();
    }
  });
}

// ---------------------------------------------------------------------------
// 11. extractThrows
// ---------------------------------------------------------------------------

export function extractThrows(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() !== SyntaxKind.ThrowStatement) return;

    const nodeId = nodeIdByNode.get(node);
    if (!nodeId) return;

    // Walk ancestors to find enclosing CatchClause
    let isInsideHandler = false;
    let handlerNodeId: string | null = null;
    let parent = node.getParent();
    while (parent) {
      if (parent.getKind() === SyntaxKind.CatchClause) {
        isInsideHandler = true;
        const varDecl = (parent as import("ts-morph").CatchClause).getVariableDeclaration();
        if (varDecl) {
          handlerNodeId = id(nodeIdByNode, varDecl) ?? null;
        }
        break;
      }
      parent = parent.getParent();
    }

    tx.insert(nodeThrows).values({
      nodeId,
      handlerNode: handlerNodeId,
      isInsideHandler,
    }).run();
  });
}

// ---------------------------------------------------------------------------
// 12. extractCalls
// ---------------------------------------------------------------------------

export function extractCalls(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() !== SyntaxKind.CallExpression) return;
    const call = node as CallExpression;
    const nodeId = nodeIdByNode.get(call);
    const expr = call.getExpression();
    const calleeId = id(nodeIdByNode, expr);
    if (!nodeId || !calleeId) return;

    const exprKind = expr.getKind();
    const isMethodCall = exprKind === SyntaxKind.PropertyAccessExpression;
    let calleeName: string | null = null;
    if (isMethodCall || exprKind === SyntaxKind.Identifier) {
      calleeName = expr.getText();
    }

    tx.insert(nodeCalls).values({
      nodeId,
      calleeNode: calleeId,
      calleeName,
      argCount: call.getArguments().length,
      isMethodCall,
      calleeIsAsync: false,
    }).run();
  });
}

// ---------------------------------------------------------------------------
// 13. extractCaptures
//
// DONE_WITH_CONCERNS: The simple heuristic of checking whether the symbol
// declaration position falls outside the function range is brittle on
// in-memory ts-morph projects without tsconfig. The implementation below
// is a best-effort pass; it may miss captures in complex patterns.
// ---------------------------------------------------------------------------

export function extractCaptures(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  const fnKinds = [
    SyntaxKind.FunctionDeclaration,
    SyntaxKind.FunctionExpression,
    SyntaxKind.ArrowFunction,
  ] as const;

  for (const kind of fnKinds) {
    sourceFile.getDescendantsOfKind(kind).forEach((fnNode) => {
      const fnNodeId = nodeIdByNode.get(fnNode);
      if (!fnNodeId) return;

      const fnStart = fnNode.getStart();
      const fnEnd = fnNode.getEnd();

      const seen = new Set<string>();
      fnNode.getDescendantsOfKind(SyntaxKind.Identifier).forEach((ident) => {
        const name = ident.getText();
        if (seen.has(name)) return;

        let declStart: number | undefined;
        try {
          const sym = ident.getSymbol();
          if (!sym) return;
          const decls = sym.getDeclarations();
          if (!decls || decls.length === 0) return;
          declStart = decls[0].getStart();
        } catch {
          return;
        }

        // If declaration is outside the function, this is a capture
        if (declStart !== undefined && (declStart < fnStart || declStart > fnEnd)) {
          seen.add(name);

          // Determine mutability: look for let declarations
          let mutable = false;
          try {
            const sym = ident.getSymbol();
            const decls = sym?.getDeclarations() ?? [];
            for (const d of decls) {
              if (d.getKind() === SyntaxKind.VariableDeclaration) {
                const varDecl = d as VariableDeclaration;
                const parent = varDecl.getParent();
                if (parent?.getKind() === SyntaxKind.VariableDeclarationList) {
                  const flags = (parent as import("ts-morph").VariableDeclarationList).getFlags();
                  // NodeFlags.Let = 1, NodeFlags.Const = 2
                  mutable = (flags & 1) !== 0;
                }
              }
            }
          } catch {
            // Ignore — mutable stays false
          }

          const declNode = ident.getSymbol()?.getDeclarations()[0];
          const declNodeId = declNode ? id(nodeIdByNode, declNode) ?? null : null;

          tx.insert(nodeCaptures).values({
            nodeId: fnNodeId,
            capturedName: name,
            declaredInNode: declNodeId,
            mutable,
          }).run();
        }
      });
    });
  }
}

// ---------------------------------------------------------------------------
// 14. extractPattern
// ---------------------------------------------------------------------------

export function extractPattern(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.ObjectBindingPattern) {
      // Object destructuring — emit a row for the pattern node itself
      const nodeId = nodeIdByNode.get(node);
      if (!nodeId) return;
      tx.insert(nodePattern).values({
        nodeId,
        patternKind: "object",
        slotKey: null,
        renameTo: null,
        defaultSmt: null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.ArrayBindingPattern) {
      const nodeId = nodeIdByNode.get(node);
      if (!nodeId) return;
      tx.insert(nodePattern).values({
        nodeId,
        patternKind: "array",
        slotKey: null,
        renameTo: null,
        defaultSmt: null,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.BindingElement) {
      const be = node as BindingElement;
      const nodeId = nodeIdByNode.get(be);
      if (!nodeId) return;

      const isDotDotDot = be.getDotDotDotToken() !== undefined;
      const propName = be.getPropertyNameNode();
      const nameNode = be.getNameNode();
      const initializer = be.getInitializer();

      let patternKind: string;
      if (isDotDotDot) {
        patternKind = "rest";
      } else if (propName) {
        patternKind = "object";
      } else {
        patternKind = "identifier";
      }

      const slotKey = propName ? propName.getText() : (nameNode ? nameNode.getText() : null);
      const renameTo = propName && nameNode ? nameNode.getText() : null;
      const defaultSmt = initializer ? initializer.getText() : null;

      tx.insert(nodePattern).values({
        nodeId,
        patternKind,
        slotKey,
        renameTo,
        defaultSmt,
      }).run();
    }
  });
}

// ---------------------------------------------------------------------------
// 15. extractBinding
// ---------------------------------------------------------------------------

export function extractBinding(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.VariableDeclaration) {
      const vd = node as VariableDeclaration;
      const nodeId = nodeIdByNode.get(vd);
      if (!nodeId) return;

      // Get binding kind from parent VariableDeclarationList
      const parent = vd.getParent();
      let bindingKind = "var";
      if (parent && parent.getKind() === SyntaxKind.VariableDeclarationList) {
        const flags = (parent as import("ts-morph").VariableDeclarationList).getFlags();
        if (flags & 2) bindingKind = "const";
        else if (flags & 1) bindingKind = "let";
      }

      const nameNode = vd.getNameNode();
      // Skip destructuring patterns — nameNode may be ObjectBindingPattern/ArrayBindingPattern
      if (nameNode.getKind() !== SyntaxKind.Identifier) return;

      const typeNode = vd.getTypeNode();
      tx.insert(nodeBinding).values({
        nodeId,
        name: nameNode.getText(),
        declaredType: typeNode ? typeNode.getText() : null,
        bindingKind,
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.Parameter) {
      const param = node as ParameterDeclaration;
      const nodeId = nodeIdByNode.get(param);
      if (!nodeId) return;
      const nameNode = param.getNameNode();
      if (!nameNode || nameNode.getKind() !== SyntaxKind.Identifier) return;
      const typeNode = param.getTypeNode();
      tx.insert(nodeBinding).values({
        nodeId,
        name: nameNode.getText(),
        declaredType: typeNode ? typeNode.getText() : null,
        bindingKind: "param",
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.FunctionDeclaration) {
      const fd = node as FunctionDeclaration;
      const nodeId = nodeIdByNode.get(fd);
      if (!nodeId) return;
      const name = fd.getName();
      if (!name) return;
      const retType = fd.getReturnTypeNode();
      tx.insert(nodeBinding).values({
        nodeId,
        name,
        declaredType: retType ? retType.getText() : null,
        bindingKind: "function",
      }).run();
      return;
    }

    if (node.getKind() === SyntaxKind.ClassDeclaration) {
      const cd = node as ClassDeclaration;
      const nodeId = nodeIdByNode.get(cd);
      if (!nodeId) return;
      const name = cd.getName();
      if (!name) return;
      tx.insert(nodeBinding).values({
        nodeId,
        name,
        declaredType: null,
        bindingKind: "class",
      }).run();
    }
  });
}

// ---------------------------------------------------------------------------
// 16. extractSignal
// ---------------------------------------------------------------------------

const CONSOLE_METHODS = new Set(["console.log", "console.warn", "console.error", "console.info"]);

export function extractSignal(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  const insertedSignalIds = new Set<string>();

  sourceFile.forEachDescendant((node) => {
    if (node.getKind() === SyntaxKind.CallExpression) {
      const call = node as CallExpression;
      const expr = call.getExpression();
      const calleeName = expr.getText();
      if (CONSOLE_METHODS.has(calleeName)) {
        const nodeId = nodeIdByNode.get(call);
        if (!nodeId || insertedSignalIds.has(nodeId)) return;
        const args = call.getArguments();
        const payload = args[0] ? args[0].getText() : "";
        tx.insert(nodeSignal).values({
          nodeId,
          signalKind: "log",
          signalPayload: payload,
        }).run();
        insertedSignalIds.add(nodeId);
      }
      return;
    }

    if (node.getKind() === SyntaxKind.ThrowStatement) {
      const thr = node as import("ts-morph").ThrowStatement;
      const nodeId = nodeIdByNode.get(thr);
      if (!nodeId || insertedSignalIds.has(nodeId)) return;
      const expr = thr.getExpression();
      if (!expr || expr.getKind() !== SyntaxKind.NewExpression) return;
      const newExpr = expr as import("ts-morph").NewExpression;
      const args = newExpr.getArguments();
      const payload = args[0] ? args[0].getText() : "";
      tx.insert(nodeSignal).values({
        nodeId,
        signalKind: "throw_message",
        signalPayload: payload,
      }).run();
      insertedSignalIds.add(nodeId);
      return;
    }

    // TODO/FIXME comments via trivia
    if (
      node.getKind() === SyntaxKind.SingleLineCommentTrivia ||
      node.getKind() === SyntaxKind.MultiLineCommentTrivia
    ) {
      const text = node.getText();
      if (text.includes("TODO") || text.includes("FIXME")) {
        const nodeId = nodeIdByNode.get(node);
        if (!nodeId || insertedSignalIds.has(nodeId)) return;
        tx.insert(nodeSignal).values({
          nodeId,
          signalKind: "todo_comment",
          signalPayload: text,
        }).run();
        insertedSignalIds.add(nodeId);
      }
    }
  });
}

// ---------------------------------------------------------------------------
// 17. extractSignalInterpolations
// ---------------------------------------------------------------------------

export function extractSignalInterpolations(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    if (node.getKind() !== SyntaxKind.TemplateExpression) return;
    const tmpl = node as TemplateExpression;
    const signalId = nodeIdByNode.get(tmpl);
    if (!signalId) return;

    const spans = tmpl.getTemplateSpans();
    spans.forEach((span, i) => {
      const expr = span.getExpression();
      const interpolatedId = id(nodeIdByNode, expr);
      if (!interpolatedId) return;
      tx.insert(signalInterpolations).values({
        signalNode: signalId,
        slotIndex: i,
        interpolatedNode: interpolatedId,
      }).run();
    });
  });
}

// ---------------------------------------------------------------------------
// Top-level dispatcher
// ---------------------------------------------------------------------------

export function extractAllCapabilities(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  extractArithmetic(tx, sourceFile, nodeIdByNode);
  extractAssigns(tx, sourceFile, nodeIdByNode);
  extractReturns(tx, sourceFile, nodeIdByNode);
  extractMemberAccess(tx, sourceFile, nodeIdByNode);
  extractNonNullAssertion(tx, sourceFile, nodeIdByNode);
  extractTruthiness(tx, sourceFile, nodeIdByNode);
  extractNarrows(tx, sourceFile, nodeIdByNode);
  extractDecides(tx, sourceFile, nodeIdByNode);
  extractIterates(tx, sourceFile, nodeIdByNode);
  extractYields(tx, sourceFile, nodeIdByNode);
  extractThrows(tx, sourceFile, nodeIdByNode);
  extractCalls(tx, sourceFile, nodeIdByNode);
  extractCaptures(tx, sourceFile, nodeIdByNode);
  extractPattern(tx, sourceFile, nodeIdByNode);
  extractBinding(tx, sourceFile, nodeIdByNode);
  extractSignal(tx, sourceFile, nodeIdByNode);
  extractSignalInterpolations(tx, sourceFile, nodeIdByNode);
}
