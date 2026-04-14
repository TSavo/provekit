import Parser from "tree-sitter";
import { Signal, SignalGenerator, ParameterType } from "./Signal";

const NAME_PATTERNS: { pattern: RegExp; type: string; contract: string }[] = [
  { pattern: /^sanitize/i, type: "sanitization", contract: "output must not contain dangerous characters that were present in input" },
  { pattern: /^validate/i, type: "validation", contract: "must reject invalid input and only return/proceed on valid input" },
  { pattern: /^ensure/i, type: "guarantee", contract: "postcondition must hold after execution or function must throw" },
  { pattern: /^verify/i, type: "verification", contract: "must return true only when the condition actually holds" },
  { pattern: /^check/i, type: "check", contract: "must inspect the condition and return a boolean reflecting reality" },
  { pattern: /^assert/i, type: "assertion", contract: "must throw if the condition does not hold" },
  { pattern: /^require/i, type: "precondition", contract: "must throw if precondition is not met" },
  { pattern: /^normalize/i, type: "normalization", contract: "output must be in canonical form regardless of input variation" },
  { pattern: /^clamp/i, type: "boundary", contract: "output must be within the specified bounds" },
  { pattern: /^bound/i, type: "boundary", contract: "output must not exceed the specified limits" },
  { pattern: /^limit/i, type: "boundary", contract: "output must not exceed the maximum" },
  { pattern: /^cap/i, type: "boundary", contract: "output must not exceed the ceiling" },
  { pattern: /^floor/i, type: "boundary", contract: "output must not go below the minimum" },
  { pattern: /^ceil/i, type: "boundary", contract: "output must not exceed the ceiling" },
  { pattern: /^filter/i, type: "filter", contract: "output must only contain elements matching the predicate" },
  { pattern: /^strip/i, type: "sanitization", contract: "output must not contain the stripped characters/patterns" },
  { pattern: /^escape/i, type: "sanitization", contract: "all special characters in output must be escaped" },
  { pattern: /^encode/i, type: "encoding", contract: "output must be valid in the target encoding" },
  { pattern: /^decode/i, type: "encoding", contract: "output must faithfully represent the encoded input" },
  { pattern: /^parse/i, type: "parsing", contract: "must return structured data matching the input format or throw on malformed input" },
  { pattern: /^serialize/i, type: "serialization", contract: "output must be deserializable back to equivalent input" },
  { pattern: /^auth/i, type: "security", contract: "must verify credentials before granting access" },
  { pattern: /^isValid/i, type: "validation", contract: "must return true only when the input is actually valid" },
  { pattern: /^is[A-Z]/i, type: "predicate", contract: "return value must accurately reflect the property being tested" },
  { pattern: /^has[A-Z]/i, type: "predicate", contract: "return value must accurately reflect presence of the property" },
  { pattern: /^can[A-Z]/i, type: "predicate", contract: "return value must accurately reflect the capability" },
  { pattern: /^should[A-Z]/i, type: "predicate", contract: "return value must accurately reflect the recommendation" },
  { pattern: /^to[A-Z]/, type: "conversion", contract: "output type must match the target format and be lossless where possible" },
  { pattern: /^from[A-Z]/, type: "conversion", contract: "must construct a valid instance from the source format" },
];

const VAR_NAME_PATTERNS: { pattern: RegExp; type: string; contract: string }[] = [
  { pattern: /^safe/i, type: "safety", contract: "value must have been validated or sanitized" },
  { pattern: /^trusted/i, type: "trust", contract: "value must have been verified against a trust source" },
  { pattern: /^clean/i, type: "sanitization", contract: "value must have had dangerous content removed" },
  { pattern: /^valid/i, type: "validation", contract: "value must have passed validation" },
  { pattern: /^normalized/i, type: "normalization", contract: "value must be in canonical form" },
  { pattern: /^max/i, type: "boundary", contract: "value must be an upper bound" },
  { pattern: /^min/i, type: "boundary", contract: "value must be a lower bound" },
];

export class FunctionNameSignalGenerator implements SignalGenerator {
  readonly name = "function-name";
  readonly async = false;

  findSignals(filePath: string, source: string, tree: Parser.Tree): Signal[] {
    const signals: Signal[] = [];
    let functionsScanned = 0;
    let namesMatched = 0;

    console.log(`[fn-name-signal] Scanning ${filePath} for semantic function/variable names...`);

    this.visitFunctions(tree.rootNode, (fnNode) => {
      functionsScanned++;
      const fnName = this.extractFunctionName(fnNode);
      if (fnName === "<anonymous>") return;

      for (const { pattern, type, contract } of NAME_PATTERNS) {
        if (!pattern.test(fnName)) continue;

        namesMatched++;
        const line = fnNode.startPosition.row + 1;
        const parameters = this.extractParameters(fnNode);
        const returnType = this.extractReturnType(fnNode);

        console.log(`[fn-name-signal] Line ${line}: ${fnName}() — name promises "${contract}" [${type}]`);

        signals.push({
          file: filePath,
          line,
          column: fnNode.startPosition.column,
          type: `name:${type}`,
          text: `Function "${fnName}" promises: ${contract}`,
          functionName: fnName,
          functionSource: fnNode.text,
          functionStartLine: fnNode.startPosition.row + 1,
          functionEndLine: fnNode.endPosition.row + 1,
          parameters,
          returnType,
          pathConditions: [],
          localTypes: {},
        });

        break;
      }
    });

    console.log(`[fn-name-signal] Scanned ${functionsScanned} functions, ${namesMatched} semantic names detected`);
    console.log(`[fn-name-signal] ${signals.length} signals from function names in ${filePath}`);
    return signals;
  }

  private visitFunctions(node: Parser.SyntaxNode, callback: (node: Parser.SyntaxNode) => void): void {
    const visited = new Set<number>();

    const visit = (n: Parser.SyntaxNode): void => {
      if (
        n.type === "function_declaration" ||
        n.type === "method_definition" ||
        n.type === "arrow_function" ||
        n.type === "function_expression"
      ) {
        if (!visited.has(n.id)) {
          visited.add(n.id);
          callback(n);
        }
      }

      if (n.type === "export_statement" && n.firstNamedChild?.type === "function_declaration") {
        if (!visited.has(n.firstNamedChild.id)) {
          visited.add(n.firstNamedChild.id);
          callback(n.firstNamedChild);
        }
      }

      if (n.type === "variable_declarator") {
        const value = n.childForFieldName("value");
        if (value && (value.type === "arrow_function" || value.type === "function_expression")) {
          if (!visited.has(value.id)) {
            visited.add(value.id);
            callback(value);
          }
        }
      }

      for (const child of n.children) {
        visit(child);
      }
    };

    visit(node);
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
}
