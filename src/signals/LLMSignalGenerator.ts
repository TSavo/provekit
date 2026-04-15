import Parser from "tree-sitter";
import { relative } from "path";
import { Signal, SignalGenerator, ParameterType } from "./Signal";
import { LLMProvider, createProvider } from "../llm";

const SYSTEM_PROMPT = `You identify verification points in source code. For each point, you output a JSON array of objects with these fields:
- line: the 1-based line number
- type: one of "invariant", "precondition", "postcondition", "security", "boundary", "temporal"
- text: a one-sentence description of what should be verified at this point
- reason: why this point matters (the programmer intent signal you detected)

Only output the JSON array. No markdown fences, no explanation.`;

const USER_PROMPT = `Analyze this source file and identify every point where a formal invariant should be verified. Look for:

1. **State transitions** — where mutable state changes (assignments to shared variables, DB writes, cache mutations)
2. **Trust boundaries** — where data crosses from untrusted to trusted (user input → query, external data → internal state)
3. **Implicit contracts** — where function names, variable names, or comments promise something the code must deliver (sanitize*, validate*, ensure*, safe*)
4. **Conservation laws** — where quantities should be conserved across operations (money in = money out, items reserved + items available = total)
5. **Temporal hazards** — where the same operation can be called twice with different results (no idempotency guard, shared mutable state between calls)
6. **Arithmetic boundaries** — division, subtraction near zero, multiplication overflow, empty collection operations
7. **Error handling gaps** — catch blocks that swallow errors, missing null checks on nullable returns, unchecked promise rejections

Focus on points the code does NOT already guard against. Skip trivial logging. Skip points already covered by type safety. Every signal you emit should be a place where a Z3 proof could find a real bug.

File: {{FILE_PATH}}

\`\`\`typescript
{{SOURCE}}
\`\`\``;

export interface LLMSignalConfig {
  model?: string;
  maxSignalsPerFile?: number;
  provider?: LLMProvider;
}

interface LLMSignalResult {
  line: number;
  type: string;
  text: string;
  reason: string;
}

export class LLMSignalGenerator implements SignalGenerator {
  readonly name = "llm";
  readonly async = true;

  private model: string;
  private maxSignalsPerFile: number;
  private provider: LLMProvider;

  constructor(config: LLMSignalConfig = {}) {
    this.model = config.model || "sonnet";
    this.maxSignalsPerFile = config.maxSignalsPerFile || 50;
    this.provider = config.provider || createProvider();
    console.log(`[llm-signal] Initialized LLMSignalGenerator (model: ${this.model}, provider: ${this.provider.name}, max: ${this.maxSignalsPerFile}/file)`);
  }

  async findSignals(filePath: string, source: string, tree: Parser.Tree): Promise<Signal[]> {
    const lineCount = source.split("\n").length;
    console.log(`[llm-signal] Scanning ${filePath} (${lineCount} lines) for verification points...`);

    const prompt = USER_PROMPT
      .replace("{{FILE_PATH}}", filePath)
      .replace("{{SOURCE}}", source);

    console.log(`[llm-signal] Sending ${prompt.length} chars to ${this.model} via ${this.provider.name}...`);
    const startTime = Date.now();

    const response = await this.provider.complete(prompt, {
      model: this.model,
      systemPrompt: SYSTEM_PROMPT,
    });
    const rawResponse = response.text;

    const elapsed = Date.now() - startTime;
    console.log(`[llm-signal] Response received in ${elapsed}ms (${rawResponse.length} chars)`);

    const llmSignals = this.parseResponse(rawResponse);
    console.log(`[llm-signal] Parsed ${llmSignals.length} raw signals from LLM response`);

    if (llmSignals.length === 0) {
      console.log(`[llm-signal] WARNING: No signals parsed. Raw response starts with: ${rawResponse.slice(0, 200)}`);
      return [];
    }

    const byType = new Map<string, number>();
    for (const s of llmSignals) {
      byType.set(s.type, (byType.get(s.type) || 0) + 1);
    }
    console.log(`[llm-signal] Signal types: ${[...byType.entries()].map(([t, c]) => `${t}=${c}`).join(", ")}`);

    const enriched = this.enrichWithAST(llmSignals, filePath, source, tree);
    const dropped = llmSignals.length - enriched.length;
    if (dropped > 0) {
      console.log(`[llm-signal] Dropped ${dropped} signals (out of range or no AST node)`);
    }

    console.log(`[llm-signal] ${enriched.length} signals enriched with AST context for ${filePath}`);
    for (const s of enriched) {
      console.log(`[llm-signal]   ${s.line}:${s.functionName} [${s.type}] ${s.text.slice(0, 80)}`);
    }

    return enriched;
  }

  private parseResponse(response: string): LLMSignalResult[] {
    const jsonMatch = response.match(/\[[\s\S]*\]/);
    if (!jsonMatch) {
      console.log(`[llm-signal] No JSON array found in response`);
      return [];
    }

    try {
      const parsed = JSON.parse(jsonMatch[0]);
      if (!Array.isArray(parsed)) {
        console.log(`[llm-signal] Parsed JSON is not an array`);
        return [];
      }
      const valid = parsed.filter((s: any) => typeof s.line === "number" && typeof s.text === "string");
      const filtered = valid.slice(0, this.maxSignalsPerFile);
      if (valid.length > this.maxSignalsPerFile) {
        console.log(`[llm-signal] Capped at ${this.maxSignalsPerFile} (${valid.length} total)`);
      }
      return filtered;
    } catch (err: any) {
      console.log(`[llm-signal] JSON parse error: ${err.message}`);
      return [];
    }
  }

  private enrichWithAST(
    llmSignals: LLMSignalResult[],
    filePath: string,
    source: string,
    tree: Parser.Tree
  ): Signal[] {
    const sourceLines = source.split("\n");
    const signals: Signal[] = [];

    for (const ls of llmSignals) {
      if (ls.line < 1 || ls.line > sourceLines.length) {
        console.log(`[llm-signal] Skipping line ${ls.line}: out of range (file has ${sourceLines.length} lines)`);
        continue;
      }

      const node = this.findNodeAtLine(tree.rootNode, ls.line - 1);
      if (!node) {
        console.log(`[llm-signal] Skipping line ${ls.line}: no AST node found`);
        continue;
      }

      const enclosingFn = this.findEnclosingFunction(node);
      const fnName = enclosingFn ? this.extractFunctionName(enclosingFn) : "<module>";
      const fnSource = enclosingFn ? enclosingFn.text : sourceLines.slice(Math.max(0, ls.line - 3), ls.line + 2).join("\n");
      const fnStart = enclosingFn ? enclosingFn.startPosition.row + 1 : ls.line;
      const fnEnd = enclosingFn ? enclosingFn.endPosition.row + 1 : ls.line;

      const parameters = enclosingFn ? this.extractParameters(enclosingFn) : [];
      const returnType = enclosingFn ? this.extractReturnType(enclosingFn) : "unknown";
      const pathConditions = enclosingFn ? this.extractPathConditions(node, enclosingFn) : [];
      const localTypes = enclosingFn ? this.extractLocalTypes(enclosingFn, node) : {};

      signals.push({
        file: filePath,
        line: ls.line,
        column: 0,
        type: `llm:${ls.type}`,
        text: `${ls.text} [${ls.reason}]`,
        functionName: fnName,
        functionSource: fnSource,
        functionStartLine: fnStart,
        functionEndLine: fnEnd,
        parameters,
        returnType,
        pathConditions,
        localTypes,
        callees: [],
        calledBy: [],
      });
    }

    return signals;
  }

  private findNodeAtLine(root: Parser.SyntaxNode, row: number): Parser.SyntaxNode | null {
    let best: Parser.SyntaxNode | null = null;

    const visit = (node: Parser.SyntaxNode): void => {
      if (node.startPosition.row <= row && node.endPosition.row >= row) {
        best = node;
        for (const child of node.children) {
          visit(child);
        }
      }
    };

    visit(root);
    return best;
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
