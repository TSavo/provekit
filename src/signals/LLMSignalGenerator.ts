import Parser from "tree-sitter";
import { Signal, SignalGenerator, ParameterType } from "./Signal";
import { LLMProvider, createProvider } from "../llm";
import { findEnclosingFunction, extractFunctionName, extractCallees } from "../parser";
import { PrincipleStore } from "../principles";

const SYSTEM_PROMPT = `You identify formal verification points in TypeScript functions. You output a JSON array. Nothing else — no markdown fences, no explanation, no prose.`;

const FUNCTION_PROMPT = `You are a formal verification signal detector. Your job: look at a TypeScript function and identify the specific lines where a Z3 SMT solver could prove or disprove a property about the code.

## What is a signal?

A signal is a line in the code where something VERIFIABLE happens — a property that can be expressed in SMT-LIB 2 and checked by Z3. Not every line is a signal. Most lines are not.

## Teaching examples

### Example 1: Function with real signals

\`\`\`typescript
function withdraw(account: Account, amount: number): number {
  const balance = account.balance;          // line 2
  if (amount <= 0) throw new Error("bad");  // line 3
  const newBalance = balance - amount;      // line 4
  account.balance = newBalance;             // line 5
  return newBalance;                        // line 6
}
\`\`\`

Good signals:
\`\`\`json
[
  {"line": 4, "type": "arithmetic", "text": "newBalance can go negative if amount > balance — no underflow guard", "reason": "subtraction without lower bound check"},
  {"line": 5, "type": "state-transition", "text": "account.balance mutation must preserve non-negative invariant", "reason": "mutable state change on shared object"},
  {"line": 3, "type": "precondition", "text": "amount <= 0 is rejected but amount > balance is not — partial guard", "reason": "incomplete input validation"}
]
\`\`\`

NOT signals in this function:
- Line 2: \`const balance = account.balance\` — just a read, nothing to prove
- Line 6: \`return newBalance\` — the return itself is fine, the bug is on line 4

### Example 2: Function with NO signals

\`\`\`typescript
function printHelp(): void {
  console.log("Usage: tool <command>");
  console.log("Commands:");
  console.log("  init     Initialize project");
  console.log("  run      Run analysis");
}
\`\`\`

Signals: \`[]\`

Why: No inputs, no outputs, no state, no computation. Pure display. There is nothing a Z3 solver could prove about this function.

### Example 3: Function with subtle signals

\`\`\`typescript
function parseConfig(raw: string): Config {
  const parsed = JSON.parse(raw);           // line 2
  const port = parsed.port || 3000;         // line 3
  const host = parsed.host || "localhost";  // line 4
  return { port, host, debug: false };      // line 5
}
\`\`\`

Good signals:
\`\`\`json
[
  {"line": 2, "type": "error-boundary", "text": "JSON.parse throws on malformed input — no try/catch", "reason": "unguarded exception source"},
  {"line": 3, "type": "type-coercion", "text": "port=0 is falsy so || 3000 silently overrides a valid port", "reason": "falsy default masks valid value"}
]
\`\`\`

NOT a signal: Line 4 — same pattern as line 3, but host="" is an unusual edge case a normal developer wouldn't encounter. Only flag the pattern ONCE with the most impactful instance.

### Example 4: Function with security signals

\`\`\`typescript
function buildQuery(table: string, filter: string): string {
  return \`SELECT * FROM \${table} WHERE \${filter}\`;
}
\`\`\`

Good signals:
\`\`\`json
[
  {"line": 2, "type": "security", "text": "string interpolation into SQL without parameterization — injection vector", "reason": "untrusted input flows directly into query"}
]
\`\`\`

## Rules

1. Every signal must point to a SPECIFIC line where the bug or property lives
2. The "text" must describe a VERIFIABLE property — something expressible as "for all inputs X, property Y holds (or fails)"
3. The "reason" must explain WHY this line matters, not WHAT the line does
4. Skip pure logging with only string literals — nothing to verify
5. Skip lines that just read variables or return values — the bug is where computation happens
6. Skip duplicate patterns — flag the most impactful instance, not every occurrence
7. If the function genuinely has nothing to verify, return an empty array \`[]\`
8. For the "type" field, use a short lowercase label that describes the category of the signal (e.g. "arithmetic", "security", "state-transition"). Use whatever label best fits — there is no fixed list.

{{PRINCIPLES}}

## Your task

Analyze this function and return a JSON array of signals. Be precise — fewer high-quality signals beat many low-quality ones. If there's nothing to verify, return \`[]\`.

Function \`{{FUNCTION_NAME}}\` in \`{{FILE_PATH}}\`:

\`\`\`typescript
{{FUNCTION_SOURCE}}
\`\`\`

Respond with ONLY a JSON array. No fences, no explanation.`;

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

function isTrivialFunction(fnNode: Parser.SyntaxNode): boolean {
  const body = fnNode.childForFieldName("body");
  if (!body) return true;

  const paramNames = new Set<string>();
  const params = fnNode.childForFieldName("parameters");
  if (params) {
    for (const child of params.namedChildren) {
      const nameNode = child.childForFieldName("pattern") || child.childForFieldName("name");
      if (nameNode) paramNames.add(nameNode.text);
    }
  }

  let hasBranch = false;
  let hasDangerousCall = false;
  let hasArithmeticOnParams = false;
  let hasPropertyAssignment = false;
  let hasTryCatch = false;
  let hasAwait = false;
  let hasMethodCallOnParam = false;
  let hasFalsyDefaultOnParam = false;
  let hasNonNullAssertion = false;

  const referencesParam = (node: Parser.SyntaxNode): boolean => {
    if (node.type === "identifier" && paramNames.has(node.text)) return true;
    for (const child of node.children) {
      if (referencesParam(child)) return true;
    }
    return false;
  };

  const visit = (node: Parser.SyntaxNode): void => {
    if (BRANCHING_TYPES.has(node.type)) hasBranch = true;
    if (node.type === "try_statement") hasTryCatch = true;
    if (node.type === "await_expression") hasAwait = true;
    if (node.type === "non_null_expression") hasNonNullAssertion = true;

    if (node.type === "call_expression") {
      const fn = node.childForFieldName("function");
      if (fn) {
        if (fn.type === "member_expression") {
          const obj = fn.childForFieldName("object");
          const method = fn.childForFieldName("property")?.text || "";
          if (DANGEROUS_CALLS.has(method)) hasDangerousCall = true;
          const fullName = `${obj?.text}.${method}`;
          if (DANGEROUS_CALLS.has(fullName)) hasDangerousCall = true;
          if (obj && referencesParam(obj)) hasMethodCallOnParam = true;
        } else if (fn.type === "identifier") {
          if (DANGEROUS_CALLS.has(fn.text)) hasDangerousCall = true;
        }
      }
    }

    if (node.type === "binary_expression") {
      const children = node.children;
      const opNode = children.find((c) =>
        c.type === "||" || c.type === "+" || c.type === "-" ||
        c.type === "*" || c.type === "/" || c.type === "%" || c.type === "**"
      );
      const opText = opNode?.type || "";

      if (["+", "-", "*", "/", "%", "**"].includes(opText)) {
        if (referencesParam(node)) hasArithmeticOnParams = true;
      }

      if (opText === "||" && referencesParam(node)) {
        hasFalsyDefaultOnParam = true;
      }
    }

    if (node.type === "assignment_expression") {
      const left = node.childForFieldName("left");
      if (left?.type === "member_expression") hasPropertyAssignment = true;
    }

    for (const child of node.children) visit(child);
  };

  visit(body);

  if (hasBranch) return false;
  if (hasDangerousCall) return false;
  if (hasTryCatch) return false;
  if (hasAwait) return false;
  if (hasArithmeticOnParams) return false;
  if (hasPropertyAssignment) return false;
  if (hasMethodCallOnParam) return false;
  if (hasFalsyDefaultOnParam) return false;
  if (hasNonNullAssertion) return false;
  return true;
}

export interface LLMSignalConfig {
  model?: string;
  provider?: LLMProvider;
  maxConcurrency?: number;
  projectRoot?: string;
}

interface LLMSignalResult {
  line: number;
  type: string;
  text: string;
  reason: string;
}

interface FunctionInfo {
  name: string;
  node: Parser.SyntaxNode;
  startLine: number;
  endLine: number;
}

export class LLMSignalGenerator implements SignalGenerator {
  readonly name = "llm";
  readonly async = true;

  private model: string;
  private provider: LLMProvider;
  private maxConcurrency: number;
  private principlesContext: string;

  constructor(config: LLMSignalConfig = {}) {
    this.model = config.model || "sonnet";
    this.provider = config.provider || createProvider();
    this.maxConcurrency = config.maxConcurrency || 5;

    const projectRoot = config.projectRoot ?? process.cwd();
    const store = new PrincipleStore(projectRoot);
    const principles = store.getAll();
    if (principles.length > 0) {
      const lines = principles.map((p) => `- **${p.id}: ${p.name}** — ${p.description.slice(0, 120)}`);
      this.principlesContext = `\n## Known verification principles\n\nThese are the ${principles.length} principles the verification engine knows about. Use them to guide what you look for — but don't limit yourself to these. If you see a bug pattern that doesn't fit any principle, flag it anyway.\n\n${lines.join("\n")}`;
    } else {
      this.principlesContext = "";
    }

    console.log(`[llm-signal] Initialized (model: ${this.model}, provider: ${this.provider.name}, concurrency: ${this.maxConcurrency}, principles: ${principles.length})`);
  }

  async findSignals(filePath: string, source: string, tree: Parser.Tree): Promise<Signal[]> {
    const functions = this.extractFunctions(tree.rootNode);
    console.log(`[llm-signal] ${filePath}: ${functions.length} functions to analyze`);

    if (functions.length === 0) return [];

    const allSignals: Signal[] = [];
    const queue = [...functions];
    let completed = 0;

    const worker = async (): Promise<void> => {
      while (queue.length > 0) {
        const fn = queue.shift()!;
        completed++;
        const pct = Math.round((completed / functions.length) * 100);
        console.log(`[llm-signal] [${completed}/${functions.length}] (${pct}%) ${fn.name} (${fn.endLine - fn.startLine + 1} lines)`);

        const signals = await this.analyzeFunction(fn, filePath, source, tree);
        allSignals.push(...signals);
      }
    };

    const workers = Array.from(
      { length: Math.min(this.maxConcurrency, functions.length) },
      () => worker()
    );
    await Promise.all(workers);

    console.log(`[llm-signal] ${allSignals.length} signals from ${functions.length} functions in ${filePath}`);
    return allSignals;
  }

  private async analyzeFunction(
    fn: FunctionInfo,
    filePath: string,
    source: string,
    tree: Parser.Tree
  ): Promise<Signal[]> {
    const prompt = FUNCTION_PROMPT
      .replace("{{FUNCTION_NAME}}", fn.name)
      .replace("{{FILE_PATH}}", filePath)
      .replace("{{FUNCTION_SOURCE}}", fn.node.text)
      .replace("{{PRINCIPLES}}", this.principlesContext);

    const startTime = Date.now();

    let rawResponse: string;
    try {
      const response = await this.provider.complete(prompt, {
        model: this.model,
        systemPrompt: SYSTEM_PROMPT,
      });
      rawResponse = response.text;
    } catch (err: any) {
      console.log(`[llm-signal] ERROR analyzing ${fn.name}: ${err.message}`);
      return [];
    }

    const elapsed = Date.now() - startTime;
    const llmSignals = this.parseResponse(rawResponse);

    if (llmSignals.length === 0) {
      console.log(`[llm-signal]   ${fn.name}: 0 signals (${elapsed}ms) — nothing to verify`);
      return [];
    }

    console.log(`[llm-signal]   ${fn.name}: ${llmSignals.length} signals (${elapsed}ms)`);

    return this.enrichSignals(llmSignals, fn, filePath, source, tree);
  }

  private extractFunctions(root: Parser.SyntaxNode): FunctionInfo[] {
    const functions: FunctionInfo[] = [];
    const visited = new Set<number>();

    const visit = (node: Parser.SyntaxNode): void => {
      let fnNode: Parser.SyntaxNode | null = null;

      if (
        node.type === "function_declaration" ||
        node.type === "method_definition"
      ) {
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
        if (name !== "<anonymous>" && !isTrivialFunction(fnNode)) {
          functions.push({
            name,
            node: fnNode,
            startLine: fnNode.startPosition.row + 1,
            endLine: fnNode.endPosition.row + 1,
          });
        }
      }

      for (const child of node.children) {
        visit(child);
      }
    };

    visit(root);
    return functions;
  }

  private parseResponse(response: string): LLMSignalResult[] {
    let text = response.trim();
    const fenceMatch = text.match(/```(?:json)?\s*([\s\S]*?)```/);
    if (fenceMatch) text = fenceMatch[1]!.trim();

    const jsonMatch = text.match(/\[[\s\S]*\]/);
    if (!jsonMatch) return [];

    try {
      const parsed = JSON.parse(jsonMatch[0]);
      if (!Array.isArray(parsed)) return [];
      return parsed.filter(
        (s: any) => typeof s.line === "number" && typeof s.text === "string"
      );
    } catch {
      return [];
    }
  }

  private enrichSignals(
    llmSignals: LLMSignalResult[],
    fn: FunctionInfo,
    filePath: string,
    source: string,
    tree: Parser.Tree
  ): Signal[] {
    const sourceLines = source.split("\n");
    const signals: Signal[] = [];
    const parameters = this.extractParameters(fn.node);
    const returnType = this.extractReturnType(fn.node);
    const callees = extractCallees(fn.node);

    for (const ls of llmSignals) {
      let absoluteLine = ls.line;
      if (absoluteLine < fn.startLine) {
        absoluteLine = ls.line + fn.startLine - 1;
      }
      if (absoluteLine < fn.startLine || absoluteLine > fn.endLine) continue;
      if (absoluteLine < 1 || absoluteLine > sourceLines.length) continue;

      const node = this.findNodeAtLine(tree.rootNode, absoluteLine - 1);
      const pathConditions = node ? this.extractPathConditions(node, fn.node) : [];
      const localTypes = node ? this.extractLocalTypes(fn.node, node) : {};

      console.log(`[llm-signal]     L${absoluteLine} [${ls.type}] ${ls.text.slice(0, 70)}`);

      signals.push({
        file: filePath,
        line: absoluteLine,
        column: 0,
        type: `llm:${ls.type || "invariant"}`,
        text: `${ls.text} [${ls.reason}]`,
        functionName: fn.name,
        functionSource: fn.node.text,
        functionStartLine: fn.startLine,
        functionEndLine: fn.endLine,
        parameters,
        returnType,
        pathConditions,
        localTypes,
        callees,
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
        for (const child of node.children) visit(child);
      }
    };
    visit(root);
    return best;
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
