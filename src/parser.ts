import Parser from "tree-sitter";
import TypeScript from "tree-sitter-typescript";

export interface ParameterType {
  name: string;
  type: string;  // the TypeScript type annotation, or "unknown"
}

export interface LogCallSite {
  line: number;
  column: number;
  logText: string;
  functionName: string;
  functionSource: string;
  functionStartLine: number;
  functionEndLine: number;
  // Enriched context from AST:
  parameters: ParameterType[];
  returnType: string;
  pathConditions: string[];
  localTypes: Record<string, string>;
  callees: string[];
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
  const parameters = extractParameters(enclosingFn);
  const returnType = extractReturnType(enclosingFn);
  const pathConditions = extractPathConditions(node, enclosingFn);
  const localTypes = extractLocalTypes(enclosingFn, node);
  const callees = extractCallees(enclosingFn);

  return {
    line: node.startPosition.row + 1,
    column: node.startPosition.column,
    logText: node.text,
    functionName: fnName,
    functionSource: enclosingFn.text,
    functionStartLine: enclosingFn.startPosition.row + 1,
    functionEndLine: enclosingFn.endPosition.row + 1,
    parameters,
    returnType,
    pathConditions,
    localTypes,
    callees,
  };
}

export function findEnclosingFunction(node: Parser.SyntaxNode): Parser.SyntaxNode | null {
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

export function extractFunctionName(node: Parser.SyntaxNode): string {
  const nameNode = node.childForFieldName("name");
  if (nameNode) return nameNode.text;

  if (node.parent?.type === "variable_declarator") {
    const varName = node.parent.childForFieldName("name");
    if (varName) return varName.text;
  }

  // Callback argument: fn((...) => { }) or obj.method((...) => { })
  if (node.parent?.type === "arguments" && node.parent.parent?.type === "call_expression") {
    const callFn = node.parent.parent.childForFieldName("function");
    if (callFn?.type === "member_expression") {
      const method = callFn.childForFieldName("property");
      const object = callFn.childForFieldName("object");
      if (method) {
        const objName = object?.type === "call_expression"
          ? extractFunctionName(object) || object.childForFieldName("function")?.text
          : object?.text;
        if (objName) return `${objName}.${method.text}`;
        return method.text;
      }
    }
    // build(async function() { }) → "build.callback"
    if (callFn?.type === "identifier") {
      return `${callFn.text}.callback`;
    }
  }

  // Walk up to find any named ancestor for context
  let current: Parser.SyntaxNode | null = node.parent;
  while (current) {
    if (current.type === "variable_declarator") {
      const name = current.childForFieldName("name");
      if (name) return name.text;
    }
    if (current.type === "pair") {
      const key = current.childForFieldName("key");
      if (key) return key.text;
    }
    if (current.type === "assignment_expression") {
      const left = current.childForFieldName("left");
      if (left?.type === "member_expression") {
        const prop = left.childForFieldName("property");
        if (prop) return prop.text;
      }
    }
    current = current.parent;
  }

  return "<anonymous>";
}

/**
 * Extract parameter names and type annotations from a function's formal parameters.
 */
function extractParameters(fnNode: Parser.SyntaxNode): ParameterType[] {
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
    // Destructured parameters
    if (child.type === "object_pattern" || child.type === "array_pattern") {
      params.push({
        name: child.text,
        type: "unknown",
      });
    }
  }

  return params;
}

/**
 * Extract the return type annotation from a function.
 */
function extractReturnType(fnNode: Parser.SyntaxNode): string {
  const returnType = fnNode.childForFieldName("return_type");
  if (returnType) return returnType.text.replace(/^:\s*/, "");
  return "unknown";
}

/**
 * Walk from the log statement UP to the function body, collecting every
 * if/else condition that must be true for execution to reach this line.
 *
 * If the log is inside `if (x > 0) { if (y !== null) { console.log(...) } }`,
 * path conditions are ["x > 0", "y !== null"].
 */
function extractPathConditions(
  logNode: Parser.SyntaxNode,
  fnNode: Parser.SyntaxNode
): string[] {
  const conditions: string[] = [];
  let current: Parser.SyntaxNode | null = logNode.parent;

  while (current && current.id !== fnNode.id) {
    // If this node is inside the consequence (then-branch) of an if_statement
    if (current.parent?.type === "if_statement") {
      const ifStmt = current.parent;
      const condition = ifStmt.childForFieldName("condition");
      const consequence = ifStmt.childForFieldName("consequence");
      const alternative = ifStmt.childForFieldName("alternative");

      if (condition) {
        if (consequence && isDescendantOf(logNode, consequence)) {
          // We're in the then-branch: condition is true
          conditions.unshift(condition.text);
        } else if (alternative && isDescendantOf(logNode, alternative)) {
          // We're in the else-branch: condition is false
          conditions.unshift(`!(${condition.text})`);
        }
      }
    }

    // Early return pattern: if (guard) return; ... console.log(...)
    // The log only executes if the guard was false
    if (current.parent?.type === "statement_block" || current.parent?.type === "program") {
      const siblings = current.parent.namedChildren;
      const myIndex = siblings.indexOf(current);

      for (let i = 0; i < myIndex; i++) {
        const sib = siblings[i];
        if (!sib) continue;
        if (sib.type === "if_statement") {
          const consequence = sib.childForFieldName("consequence");
          if (consequence && containsReturn(consequence)) {
            const cond = sib.childForFieldName("condition");
            if (cond) {
              // This guard returned early — if we're still here, the guard was false
              conditions.unshift(`!(${cond.text})`);
            }
          }
        }
      }
    }

    current = current.parent;
  }

  return conditions;
}

/**
 * Extract local variable declarations with type annotations that appear
 * BEFORE the log statement in the function body.
 */
function extractLocalTypes(
  fnNode: Parser.SyntaxNode,
  logNode: Parser.SyntaxNode
): Record<string, string> {
  const types: Record<string, string> = {};
  const logLine = logNode.startPosition.row;

  function visit(node: Parser.SyntaxNode): void {
    // Only look at declarations before the log statement
    if (node.startPosition.row >= logLine) return;

    if (node.type === "variable_declarator") {
      const nameNode = node.childForFieldName("name");
      const typeNode = node.childForFieldName("type");
      const valueNode = node.childForFieldName("value");

      if (nameNode) {
        if (typeNode) {
          types[nameNode.text] = typeNode.text.replace(/^:\s*/, "");
        } else if (valueNode) {
          // Infer type from value expression
          const inferred = inferType(valueNode);
          if (inferred) types[nameNode.text] = inferred;
        }
      }
    }

    for (const child of node.namedChildren) {
      visit(child);
    }
  }

  const body = fnNode.childForFieldName("body");
  if (body) visit(body);

  return types;
}

function inferType(valueNode: Parser.SyntaxNode): string | null {
  switch (valueNode.type) {
    case "number": return "number";
    case "string": case "template_string": return "string";
    case "true": case "false": return "boolean";
    case "null": return "null";
    case "array": return "array";
    case "object": return "object";
    case "call_expression": {
      // Try to extract return type from known patterns
      const fn = valueNode.childForFieldName("function");
      if (fn?.type === "member_expression") {
        const prop = fn.childForFieldName("property");
        // Common patterns: .toString() → string, .length → number
        if (prop?.text === "toString" || prop?.text === "toFixed" || prop?.text === "toUpperCase") return "string";
        if (prop?.text === "length" || prop?.text === "indexOf") return "number";
      }
      return null;
    }
    case "binary_expression": {
      const op = valueNode.children.find(c => c.type === "+" || c.type === "-" || c.type === "*" || c.type === "/");
      if (op) return "number";
      return null;
    }
    default: return null;
  }
}

function isDescendantOf(node: Parser.SyntaxNode, ancestor: Parser.SyntaxNode): boolean {
  let current: Parser.SyntaxNode | null = node;
  while (current) {
    if (current.id === ancestor.id) return true;
    current = current.parent;
  }
  return false;
}

function containsReturn(node: Parser.SyntaxNode): boolean {
  if (node.type === "return_statement") return true;
  for (const child of node.namedChildren) {
    if (containsReturn(child)) return true;
  }
  return false;
}

export function extractCallees(fnNode: Parser.SyntaxNode): string[] {
  const callees = new Set<string>();

  const visit = (node: Parser.SyntaxNode): void => {
    if (node.type === "call_expression") {
      const fn = node.childForFieldName("function");
      if (fn) {
        if (fn.type === "identifier") {
          callees.add(fn.text);
        } else if (fn.type === "member_expression") {
          const prop = fn.childForFieldName("property");
          if (prop) callees.add(prop.text);
        }
      }
    }
    for (const child of node.children) {
      visit(child);
    }
  };

  const body = fnNode.childForFieldName("body");
  if (body) visit(body);
  return [...callees];
}
