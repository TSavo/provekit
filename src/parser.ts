import Parser from "tree-sitter";
import TypeScript from "tree-sitter-typescript";

export interface LogCallSite {
  line: number;
  column: number;
  logText: string;
  functionName: string;
  functionSource: string;
  functionStartLine: number;
  functionEndLine: number;
}

const LOG_OBJECTS = new Set([
  "console",
  "logger",
  "log",
]);

const LOG_METHODS = new Set([
  "log",
  "info",
  "debug",
  "warn",
  "error",
  "trace",
]);

export function parseFile(source: string): Parser.Tree {
  const parser = new Parser();
  parser.setLanguage(TypeScript.typescript);
  return parser.parse(source);
}

export function findLogStatements(tree: Parser.Tree, source: string): LogCallSite[] {
  const results: LogCallSite[] = [];
  const rootNode = tree.rootNode;

  function visit(node: Parser.SyntaxNode): void {
    if (node.type === "call_expression") {
      const callSite = tryExtractLogCall(node, source);
      if (callSite) {
        results.push(callSite);
      }
    }
    for (const child of node.children) {
      visit(child);
    }
  }

  visit(rootNode);
  return results;
}

function tryExtractLogCall(
  node: Parser.SyntaxNode,
  source: string
): LogCallSite | null {
  const fn = node.childForFieldName("function");
  if (!fn || fn.type !== "member_expression") return null;

  const object = fn.childForFieldName("object");
  const property = fn.childForFieldName("property");
  if (!object || !property) return null;

  const objectName = object.text;
  const methodName = property.text;

  if (!LOG_OBJECTS.has(objectName) || !LOG_METHODS.has(methodName)) return null;

  const enclosingFn = findEnclosingFunction(node);
  if (!enclosingFn) return null;

  const fnName = extractFunctionName(enclosingFn);

  return {
    line: node.startPosition.row + 1,
    column: node.startPosition.column,
    logText: node.text,
    functionName: fnName,
    functionSource: enclosingFn.text,
    functionStartLine: enclosingFn.startPosition.row + 1,
    functionEndLine: enclosingFn.endPosition.row + 1,
  };
}

function findEnclosingFunction(node: Parser.SyntaxNode): Parser.SyntaxNode | null {
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
    // exported function: export function foo() {}
    if (
      current.type === "export_statement" &&
      current.firstNamedChild?.type === "function_declaration"
    ) {
      return current.firstNamedChild;
    }
    current = current.parent;
  }
  return null;
}

function extractFunctionName(node: Parser.SyntaxNode): string {
  const nameNode = node.childForFieldName("name");
  if (nameNode) return nameNode.text;

  // arrow function assigned to variable: const foo = () => {}
  if (node.parent?.type === "variable_declarator") {
    const varName = node.parent.childForFieldName("name");
    if (varName) return varName.text;
  }

  return "<anonymous>";
}
