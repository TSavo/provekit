import Parser from "tree-sitter";
import { Signal, SignalGenerator, ParameterType } from "./Signal";
import { findEnclosingFunction, extractFunctionName, extractCallees } from "../parser";

const BRANCHING_TYPES = new Set([
  "if_statement", "for_statement", "for_in_statement", "while_statement",
  "do_statement", "switch_statement", "ternary_expression",
]);

const DANGEROUS_CALLS = new Set([
  "execSync", "exec", "spawn", "spawnSync", "eval", "Function",
  "JSON.parse", "JSON.stringify",
  "readFileSync", "readFile", "writeFileSync", "writeFile",
  "mkdirSync", "unlinkSync", "existsSync",
  "fetch", "request", "require",
  "query", "execute",
]);

interface FunctionInfo {
  name: string;
  node: Parser.SyntaxNode;
  startLine: number;
  endLine: number;
}

interface RawSignal {
  line: number;
  type: string;
  text: string;
  node: Parser.SyntaxNode;
}

export class ASTSignalGenerator implements SignalGenerator {
  readonly name = "ast";
  readonly async = false;

  findSignals(filePath: string, source: string, tree: Parser.Tree): Signal[] {
    const functions = this.extractFunctions(tree.rootNode);
    const allSignals: Signal[] = [];
    let trivialCount = 0;

    for (const fn of functions) {
      const raw = this.analyzeFunction(fn);
      if (raw.length === 0) {
        trivialCount++;
        continue;
      }

      const params = this.extractParameters(fn.node);
      const returnType = this.extractReturnType(fn.node);
      const callees = extractCallees(fn.node);

      for (const sig of raw) {
        const pathConditions = this.extractPathConditions(sig.node, fn.node);
        const localTypes = this.extractLocalTypes(fn.node, sig.node);

        allSignals.push({
          file: filePath,
          line: sig.line,
          column: sig.node.startPosition.column,
          type: `ast:${sig.type}`,
          text: sig.text,
          functionName: fn.name,
          functionSource: fn.node.text,
          functionStartLine: fn.startLine,
          functionEndLine: fn.endLine,
          parameters: params,
          returnType,
          pathConditions,
          localTypes,
          callees,
          calledBy: [],
        });
      }
    }

    console.log(`[ast-signal] ${filePath}: ${functions.length} functions, ${allSignals.length} signals, ${trivialCount} trivial skipped`);
    return allSignals;
  }

  private analyzeFunction(fn: FunctionInfo): RawSignal[] {
    const body = fn.node.childForFieldName("body");
    if (!body) return [];

    const paramNames = new Set<string>();
    const params = fn.node.childForFieldName("parameters");
    if (params) {
      for (const child of params.namedChildren) {
        const nameNode = child.childForFieldName("pattern") || child.childForFieldName("name");
        if (nameNode) paramNames.add(nameNode.text);
      }
    }

    const signals: RawSignal[] = [];
    const seen = new Set<number>();

    const emit = (node: Parser.SyntaxNode, type: string, text: string) => {
      const line = node.startPosition.row + 1;
      if (seen.has(line)) return;
      seen.add(line);
      signals.push({ line, type, text, node });
    };

    const referencesParam = (node: Parser.SyntaxNode): boolean => {
      if (node.type === "identifier" && paramNames.has(node.text)) return true;
      for (const child of node.children) {
        if (referencesParam(child)) return true;
      }
      return false;
    };

    const visit = (node: Parser.SyntaxNode): void => {
      if (BRANCHING_TYPES.has(node.type)) {
        if (node.type === "if_statement") {
          const cond = node.childForFieldName("condition");
          const alt = node.childForFieldName("alternative");
          emit(node, "branch", alt
            ? `if/else branch — both paths must be correct [condition: ${cond?.text.slice(0, 60)}]`
            : `if without else — implicit fall-through path when condition is false [condition: ${cond?.text.slice(0, 60)}]`);
        } else if (node.type === "for_statement" || node.type === "for_in_statement" || node.type === "while_statement" || node.type === "do_statement") {
          emit(node, "loop", `loop — iteration correctness, termination, empty-collection behavior`);
        } else if (node.type === "switch_statement") {
          emit(node, "branch", `switch — exhaustiveness, fall-through between cases`);
        } else if (node.type === "ternary_expression") {
          emit(node, "branch", `ternary — both branches must produce valid result [${node.text.slice(0, 60)}]`);
        }
      }

      if (node.type === "try_statement") {
        const handler = node.childForFieldName("handler");
        const handlerBody = handler?.childForFieldName("body");
        const isEmpty = handlerBody && handlerBody.namedChildren.length === 0;
        emit(node, "error-handling", isEmpty
          ? `try/catch with empty catch — errors silently swallowed`
          : `try/catch — error handling correctness, exception types`);
      }

      if (node.type === "await_expression") {
        emit(node, "async", `await — async error propagation, rejection handling [${node.text.slice(0, 60)}]`);
      }

      if (node.type === "non_null_expression") {
        emit(node, "assertion", `non-null assertion ! — developer assumes non-null, may be wrong [${node.text.slice(0, 40)}]`);
      }

      if (node.type === "call_expression") {
        const fn = node.childForFieldName("function");
        if (fn?.type === "member_expression") {
          const obj = fn.childForFieldName("object");
          const method = fn.childForFieldName("property")?.text || "";
          const fullName = `${obj?.text}.${method}`;

          if (DANGEROUS_CALLS.has(method) || DANGEROUS_CALLS.has(fullName)) {
            emit(node, "dangerous-call", `${fullName} — external I/O, trust boundary crossing [${node.text.slice(0, 60)}]`);
          } else if (obj && referencesParam(obj)) {
            emit(node, "param-method", `method call on parameter: ${fullName}() — input transformation, edge cases [${node.text.slice(0, 60)}]`);
          }
        } else if (fn?.type === "identifier" && DANGEROUS_CALLS.has(fn.text)) {
          emit(node, "dangerous-call", `${fn.text}() — external I/O, trust boundary [${node.text.slice(0, 60)}]`);
        }
      }

      if (node.type === "binary_expression") {
        const children = node.children;
        const opNode = children.find((c) =>
          ["+", "-", "*", "/", "%", "**", "||"].includes(c.type)
        );
        const opText = opNode?.type || "";

        if (["+", "-", "*", "/", "%", "**"].includes(opText) && referencesParam(node)) {
          emit(node, "arithmetic", `arithmetic on input — overflow, underflow, division-by-zero [${node.text.slice(0, 60)}]`);
        }

        if (opText === "||" && referencesParam(node)) {
          emit(node, "falsy-default", `|| on input — falsy values (0, "", false) silently replaced [${node.text.slice(0, 60)}]`);
        }
      }

      if (node.type === "assignment_expression") {
        const left = node.childForFieldName("left");
        if (left?.type === "member_expression") {
          emit(node, "mutation", `property mutation — shared state change [${node.text.slice(0, 60)}]`);
        }
      }

      if (node.type === "throw_statement") {
        emit(node, "throw", `throw — creates caller obligation to handle [${node.text.slice(0, 60)}]`);
      }

      for (const child of node.children) visit(child);
    };

    visit(body);
    return signals;
  }

  private extractFunctions(root: Parser.SyntaxNode): FunctionInfo[] {
    const functions: FunctionInfo[] = [];
    const visited = new Set<number>();

    const visit = (node: Parser.SyntaxNode): void => {
      let fnNode: Parser.SyntaxNode | null = null;

      if (node.type === "function_declaration" || node.type === "method_definition") {
        fnNode = node;
      } else if (node.type === "export_statement" && node.firstNamedChild?.type === "function_declaration") {
        fnNode = node.firstNamedChild;
      } else if (node.type === "variable_declarator") {
        const value = node.childForFieldName("value");
        if (value && (value.type === "arrow_function" || value.type === "function_expression")) {
          fnNode = value;
        }
      }

      if (fnNode && !visited.has(fnNode.id)) {
        visited.add(fnNode.id);
        const name = extractFunctionName(fnNode);
        if (name !== "<anonymous>") {
          functions.push({
            name,
            node: fnNode,
            startLine: fnNode.startPosition.row + 1,
            endLine: fnNode.endPosition.row + 1,
          });
        }
      }

      for (const child of node.children) visit(child);
    };

    visit(root);
    return functions;
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
