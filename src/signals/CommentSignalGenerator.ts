import Parser from "tree-sitter";
import { Signal, SignalGenerator, ParameterType } from "./Signal";

export class CommentSignalGenerator implements SignalGenerator {
  readonly name = "comment";
  readonly async = false;

  findSignals(filePath: string, source: string, tree: Parser.Tree): Signal[] {
    const signals: Signal[] = [];
    let totalComments = 0;
    let skippedNoFunction = 0;
    let skippedTooling = 0;

    console.log(`[comment-signal] Scanning ${filePath} — every comment is programmer intent`);

    this.visitComments(tree.rootNode, (commentNode) => {
      totalComments++;
      const raw = commentNode.text;
      const cleanText = raw
        .replace(/^\/\/\s*/, "")
        .replace(/^\/\*\s*/, "")
        .replace(/\s*\*\/$/, "")
        .replace(/^\s*\*\s?/gm, "")
        .trim();

      if (cleanText.length < 3) return;

      if (/^(?:eslint|prettier|tslint|@ts-|istanbul|c8|noinspection)/i.test(cleanText)) {
        skippedTooling++;
        return;
      }

      const line = commentNode.startPosition.row + 1;
      const enclosingFn = this.findEnclosingFunction(commentNode);

      if (!enclosingFn) {
        skippedNoFunction++;
        return;
      }

      const fnName = this.extractFunctionName(enclosingFn);
      const parameters = this.extractParameters(enclosingFn);
      const returnType = this.extractReturnType(enclosingFn);
      const pathConditions = this.extractPathConditions(commentNode, enclosingFn);
      const localTypes = this.extractLocalTypes(enclosingFn, commentNode);

      console.log(`[comment-signal] Line ${line} in ${fnName}(): "${cleanText.slice(0, 70)}"`);

      signals.push({
        file: filePath,
        line,
        column: commentNode.startPosition.column,
        type: "comment",
        text: cleanText,
        functionName: fnName,
        functionSource: enclosingFn.text,
        functionStartLine: enclosingFn.startPosition.row + 1,
        functionEndLine: enclosingFn.endPosition.row + 1,
        parameters,
        returnType,
        pathConditions,
        localTypes,
      });
    });

    console.log(`[comment-signal] ${totalComments} comments, ${signals.length} signals, ${skippedNoFunction} module-scope, ${skippedTooling} tooling directives`);
    return signals;
  }

  private visitComments(node: Parser.SyntaxNode, callback: (node: Parser.SyntaxNode) => void): void {
    if (node.type === "comment") {
      callback(node);
    }
    for (const child of node.children) {
      this.visitComments(child, callback);
    }
  }

  private findEnclosingFunction(node: Parser.SyntaxNode): Parser.SyntaxNode | null {
    let current: Parser.SyntaxNode | null = node.parent;
    while (current) {
      if (
        current.type === "function_declaration" ||
        current.type === "method_definition" ||
        current.type === "arrow_function" ||
        current.type === "function_expression" ||
        current.type === "function"
      ) {
        return current;
      }
      if (current.type === "export_statement" && current.firstNamedChild?.type === "function_declaration") {
        return current.firstNamedChild;
      }
      current = current.parent;
    }
    return null;
  }

  private extractFunctionName(node: Parser.SyntaxNode): string {
    const nameNode = node.childForFieldName("name");
    if (nameNode) return nameNode.text;
    if (node.parent?.type === "variable_declarator") {
      const varName = node.parent.childForFieldName("name");
      if (varName) return varName.text;
    }
    return "<anonymous>";
  }

  private extractParameters(fnNode: Parser.SyntaxNode): ParameterType[] {
    const params: ParameterType[] = [];
    const paramsNode = fnNode.childForFieldName("parameters");
    if (!paramsNode) return params;

    for (const child of paramsNode.namedChildren) {
      if (child.type === "required_parameter" || child.type === "optional_parameter") {
        const nameNode = child.childForFieldName("pattern") || child.childForFieldName("name");
        const typeNode = child.childForFieldName("type");
        params.push({
          name: nameNode?.text || "?",
          type: typeNode ? typeNode.text.replace(/^:\s*/, "") : "unknown",
        });
      }
    }
    return params;
  }

  private extractReturnType(fnNode: Parser.SyntaxNode): string {
    const returnType = fnNode.childForFieldName("return_type");
    if (returnType) return returnType.text.replace(/^:\s*/, "");
    return "unknown";
  }

  private extractPathConditions(node: Parser.SyntaxNode, fnNode: Parser.SyntaxNode): string[] {
    const conditions: string[] = [];
    let current: Parser.SyntaxNode | null = node.parent;

    while (current && current.id !== fnNode.id) {
      if (current.parent?.type === "if_statement") {
        const ifStmt = current.parent;
        const condition = ifStmt.childForFieldName("condition");
        const consequence = ifStmt.childForFieldName("consequence");
        const alternative = ifStmt.childForFieldName("alternative");

        if (condition) {
          if (consequence && this.isDescendantOf(node, consequence)) {
            conditions.unshift(condition.text);
          } else if (alternative && this.isDescendantOf(node, alternative)) {
            conditions.unshift(`!(${condition.text})`);
          }
        }
      }
      current = current.parent;
    }

    return conditions;
  }

  private extractLocalTypes(fnNode: Parser.SyntaxNode, targetNode: Parser.SyntaxNode): Record<string, string> {
    const types: Record<string, string> = {};
    const targetLine = targetNode.startPosition.row;

    const visit = (node: Parser.SyntaxNode): void => {
      if (node.startPosition.row >= targetLine) return;
      if (node.type === "variable_declarator") {
        const nameNode = node.childForFieldName("name");
        const typeNode = node.childForFieldName("type");
        if (nameNode && typeNode) {
          types[nameNode.text] = typeNode.text.replace(/^:\s*/, "");
        }
      }
      for (const child of node.namedChildren) visit(child);
    };

    const body = fnNode.childForFieldName("body");
    if (body) visit(body);
    return types;
  }

  private isDescendantOf(node: Parser.SyntaxNode, ancestor: Parser.SyntaxNode): boolean {
    let current: Parser.SyntaxNode | null = node;
    while (current) {
      if (current.id === ancestor.id) return true;
      current = current.parent;
    }
    return false;
  }
}
