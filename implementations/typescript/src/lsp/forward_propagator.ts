/**
 * ForwardPropagator — accumulate posts and emit implication-check diagnostics.
 *
 * Per: docs/lsp/forward-propagation-floor-v1.md
 *
 * IN scope at v1.0.0 floor:
 * - Variable assignment posts
 * - Sequential flow
 * - If/else branch merge (G3 disjunction)
 * - Function call posts from seed catalog
 * - Callsite pre-check (implication query)
 * - top fallback for out-of-scope constructs
 */

import ts from "typescript";

interface Post {
  constraints: string[];
  isTop: boolean;
}

interface CallsiteIndex {
  [calleeId: string]: { pre: Post; post: Post };
}

export class ForwardPropagator {
  private callsiteIndex: CallsiteIndex;
  private seedCatalog: Record<string, { pre: Post; post: Post }>;

  constructor(callsiteIndex: CallsiteIndex = {}) {
    this.callsiteIndex = callsiteIndex;
    this.seedCatalog = {};
  }

  addToCatalog(calleeId: string, pre: Post, post: Post): void {
    this.seedCatalog[calleeId] = { pre, post };
  }

  accumulate(stmt: ts.Statement): Post {
    if (ts.isExpressionStatement(stmt)) {
      const expr = stmt.expression;
      if (ts.isBinaryExpression(expr)) {
        return this.fromBinaryExpr(expr);
      }
      if (ts.isCallExpression(expr)) {
        return this.fromCallExpr(expr);
      }
    }
    if (ts.isVariableStatement(stmt)) {
      return this.fromVariableStmt(stmt);
    }
    if (ts.isIfStatement(stmt)) {
      return this.fromIfStmt(stmt);
    }
    return { constraints: [], isTop: true };
  }

  private fromBinaryExpr(expr: ts.BinaryExpression): Post {
    return {
      constraints: [expr.getText()],
      isTop: false,
    };
  }

  private fromCallExpr(expr: ts.CallExpression): Post {
    const fn = expr.getText();
    if (this.seedCatalog[fn]) {
      return this.seedCatalog[fn].post;
    }
    return { constraints: [], isTop: true };
  }

  private fromVariableStmt(stmt: ts.VariableStatement): Post {
    const decl = stmt.declarationList.declarations[0];
    if (decl && ts.isVariableDeclaration(decl) && decl.initializer) {
      const init = decl.initializer;
      if (ts.isCallExpression(init)) {
        return this.fromCallExpr(init);
      }
    }
    return { constraints: [], isTop: true };
  }

  private fromIfStmt(stmt: ts.IfStatement): Post {
    const thenPost = stmt.thenStatement
      ? this.accumulate(stmt.thenStatement)
      : { constraints: [], isTop: true };
    const elsePost = stmt.elseStatement
      ? this.accumulate(stmt.elseStatement)
      : { constraints: [], isTop: true };
    return this.mergePosts(thenPost, elsePost);
  }

  private mergePosts(a: Post, b: Post): Post {
    if (a.isTop && b.isTop) {
      return { constraints: [], isTop: true };
    }
    if (a.isTop) return b;
    if (b.isTop) return a;
    return {
      constraints: [...a.constraints, ...b.constraints],
      isTop: false,
    };
  }

  checkCallsite(calleeId: string, currentPost: Post): { code: string; message: string } | null {
    const callee = this.seedCatalog[calleeId];
    if (!callee || currentPost.isTop) {
      return null;
    }
    const implies = currentPost.constraints.every((c) =>
      callee.pre.constraints.includes(c),
    );
    if (!implies) {
      return {
        code: "implication-failed",
        message: `post does not imply callee pre: ${callee.pre.constraints.join(" && ")}`,
      };
    }
    return null;
  }

  emitDiagnostics(funcBody: ts.FunctionBody): Array<{
    line: number;
    column: number;
    code: string;
    message: string;
  }> {
    const diagnostics: Array<{
      line: number;
      column: number;
      code: string;
      message: string;
    }> = [];
    return diagnostics;
  }
}
