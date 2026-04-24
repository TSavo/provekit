import Parser from "tree-sitter";
import { TemplateResult } from "./Template";
import { PrincipleStore, Principle, ASTPattern } from "../principles";
import { SmtBinding } from "../contracts";

export class TemplateEngine {
  private principles: Principle[];

  constructor(projectRoot: string) {
    const store = new PrincipleStore(projectRoot);
    this.principles = store.getAll().filter((p) => p.astPatterns && p.astPatterns.length > 0);
    console.log(`[template-engine] Loaded ${this.principles.length} principles with AST patterns`);
  }

  generateProofs(fnNode: Parser.SyntaxNode, functionName: string, filePath: string): TemplateResult[] {
    const results: TemplateResult[] = [];
    const seen = new Set<number>();

    const paramNames = new Set<string>();
    const params = fnNode.childForFieldName("parameters");
    if (params) {
      for (const child of params.namedChildren) {
        const nameNode = child.childForFieldName("pattern") || child.childForFieldName("name");
        if (nameNode) paramNames.add(nameNode.text);
      }
    }

    const body = fnNode.childForFieldName("body");
    if (!body) return [];

    const visit = (node: Parser.SyntaxNode): void => {
      const pathConditions = this.extractPathConditions(node, fnNode);

      for (const principle of this.principles) {
        for (const pattern of principle.astPatterns!) {
          const match = this.matchPattern(node, pattern, paramNames, pathConditions, fnNode);
          if (!match) continue;

          const line = node.startPosition.row + 1;
          if (seen.has(line)) continue;
          seen.add(line);

          const guarded = match._guarded === "true";
          const smt2 = this.instantiateTemplate(principle, match, functionName, line);
          if (!smt2) {
            if (guarded) {
              console.log(`[template-engine]   L${line} ${principle.id}: guarded but no proof template — skipped`);
            }
            continue;
          }

          const mode = guarded ? "PROOF" : "VIOLATION";
          console.log(`[template-engine]   L${line} ${principle.id}: ${mode}`);

          results.push({
            signalLine: line,
            signalType: principle.id,
            smt2,
            claim: `${principle.name}: ${principle.description.slice(0, 100)}`,
            principle: principle.id,
            confidence: principle.confidence || "low",
            bindings: this.extractBindings(node, match, smt2, line),
          });
        }
      }

      for (const child of node.children) visit(child);
    };

    visit(body);
    return results;
  }

  private matchPattern(
    node: Parser.SyntaxNode,
    pattern: ASTPattern,
    paramNames: Set<string>,
    pathConditions: string[],
    fnNode: Parser.SyntaxNode
  ): Record<string, string> | null {
    if (node.type !== pattern.nodeType) return null;

    const vars: Record<string, string> = {};

    if (pattern.operator) {
      const opNode = node.children.find((c) => c.type === pattern.operator);
      if (!opNode) return null;
    }

    if (pattern.method) {
      const fn = node.childForFieldName("function");
      if (!fn) return null;
      const methodName = fn.type === "member_expression"
        ? fn.childForFieldName("property")?.text
        : fn.text;
      if (methodName !== pattern.method) return null;

      if (methodName === "execSync" || methodName === "exec" || methodName === "spawn" || methodName === "spawnSync") {
        if (this.paramOnlyInStdin(node, paramNames)) {
          vars._guarded = "true";
        }
        if (this.commandIsStaticString(node)) {
          vars._guarded = "true";
        }
      }
    }

    if (pattern.requiresParamRef) {
      if (!this.referencesParam(node, paramNames)) return null;
    }

    const guarded = pattern.guardPatterns && pattern.guardPatterns.length > 0 &&
      this.hasGuard(node, pattern.guardPatterns, pathConditions);
    if (guarded) vars._guarded = "true";

    if (pattern.nodeType === "binary_expression") {
      const left = node.childForFieldName("left");
      const right = node.childForFieldName("right");

      if ((pattern.operator === "/" || pattern.operator === "%") && right) {
        if (right.type === "number" && right.text !== "0") {
          vars._guarded = "true";
        }
        if (!this.referencesParam(right, paramNames) && right.type !== "identifier") {
          vars._guarded = "true";
        }
      }

      if (pattern.operator === "||" && left && right) {
        if (right.type === "string" || right.type === "template_string") {
          vars._guarded = "true";
        }
        if (right.type === "array" && right.namedChildren.length === 0) {
          vars._guarded = "true";
        }
        if (left.type === "call_expression" && right.type === "call_expression") {
          vars._guarded = "true";
        }
        if (this.isBooleanExpression(left) && this.isBooleanExpression(right)) {
          vars._guarded = "true";
        }
      }

      vars.left = this.extractVarName(left);
      vars.right = this.extractVarName(right);
      vars.numerator = vars.left;
      vars.denominator = vars.right;
      vars.param = this.findParamRef(node, paramNames) || vars.left;
    }

    if (pattern.nodeType === "non_null_expression") {
      const expr = node.firstNamedChild;
      vars.value = this.extractVarName(expr);
      if (expr && this.isArrayIndexGuardedByLoop(expr, pathConditions)) {
        vars._guarded = "true";
      }
      if (expr && this.isGuardedByLengthCheck(expr, pathConditions)) {
        vars._guarded = "true";
      }
      if (expr && this.isGuardedByNullCheckInConditions(expr, pathConditions)) {
        vars._guarded = "true";
      }
      if (expr && this.isMapGetAfterSetOrHas(expr, node, fnNode)) {
        vars._guarded = "true";
      }
      if (expr && this.isWrappedByTruthyCheck(expr, pathConditions)) {
        vars._guarded = "true";
      }
      if (expr && this.isSplitIndexZero(expr)) {
        vars._guarded = "true";
      }
    }

    if (pattern.nodeType === "try_statement") {
      const handler = node.childForFieldName("handler");
      const handlerBody = handler?.childForFieldName("body");
      if (!handlerBody || handlerBody.namedChildren.length > 0) return null;
      const tryBody = node.childForFieldName("body");
      if (tryBody && this.isIntentionalCleanup(tryBody)) {
        vars._guarded = "true";
      }
    }

    if (pattern.nodeType === "await_expression") {
      if (this.isInsideTryCatch(node, fnNode)) return null;
      if (this.callersAlwaysCatch(fnNode)) {
        vars._guarded = "true";
      }
    }

    if (pattern.nodeType === "throw_statement") {
      if (this.isInsideTryCatch(node, fnNode)) return null;
      if (this.isInsideCatchHandler(node, fnNode)) return null;
      if (this.isValidationGuard(node, fnNode)) return null;
      if (this.callersAlwaysCatch(fnNode)) {
        vars._guarded = "true";
      }
    }

    if (pattern.nodeType === "call_expression" && (pattern.method === "find" || pattern.method === "match")) {
      if (this.resultIsCheckedAfterCall(node, fnNode)) {
        vars._guarded = "true";
      }
      if (this.resultUsedWithOptionalChaining(node)) {
        vars._guarded = "true";
      }
    }

    if (pattern.nodeType === "if_statement") {
      const consequence = node.childForFieldName("consequence");
      const alternative = node.childForFieldName("alternative");
      const condition = node.childForFieldName("condition");
      if (!consequence || !condition) return null;

      if (pattern.nodeType === "if_statement" && !alternative) {
        vars.condition = condition.text.slice(0, 50);
        const modified = this.findModifiedVars(consequence);
        if (modified.length > 0) vars.var = modified[0]!;
      } else {
        return null;
      }
    }

    if (pattern.nodeType === "switch_statement") {
      const body = node.childForFieldName("body");
      if (!body) return null;
      if (body.children.some((c) => c.type === "switch_default")) return null;
    }

    if (pattern.nodeType === "for_in_statement" || pattern.nodeType === "for_statement" || pattern.nodeType === "while_statement") {
      const body = node.childForFieldName("body");
      if (body) {
        const acc = this.findAccumulator(body);
        if (acc) vars.accumulator = acc;
      }
      if (pattern.nodeType === "while_statement") {
        const condition = node.childForFieldName("condition");
        if (condition && body && this.bodyModifiesConditionVars(condition, body)) return null;
      }
    }

    if (pattern.nodeType === "assignment_expression") {
      const left = node.childForFieldName("left");
      if (left?.type !== "member_expression") return null;
      const prop = left.childForFieldName("property");
      vars.prop = prop?.text || "prop";
    }

    if (pattern.nodeType === "call_expression" && pattern.method === "reduce") {
      const args = node.childForFieldName("arguments");
      if (args && args.namedChildren.length >= 2) return null;
    }

    return vars;
  }

  private instantiateTemplate(
    principle: Principle,
    vars: Record<string, string>,
    functionName: string,
    line: number
  ): string | null {
    const guarded = vars._guarded === "true";
    const template = guarded ? principle.smt2ProofTemplate : principle.smt2Template;
    if (!template) {
      if (guarded && principle.smt2Template) return null;
      return null;
    }

    const tag = guarded ? "PROVEN" : "VIOLATION";
    let smt2 = template;
    for (const [key, value] of Object.entries(vars)) {
      if (key.startsWith("_")) continue;
      const safeName = value.replace(/[^a-zA-Z0-9_]/g, "_").slice(0, 30) || key;
      smt2 = smt2.replace(new RegExp(`\\{\\{${key}\\}\\}`, "g"), safeName);
    }

    smt2 = smt2.replace(/\{\{[a-zA-Z_]+\}\}/g, (match) => {
      const name = match.slice(2, -2);
      return `${name}_${line}`;
    });

    return `; PRINCIPLE: ${principle.id}\n; LINE: ${line}\n; ${tag}: ${principle.name} at ${functionName}\n${smt2}`;
  }

  private referencesParam(node: Parser.SyntaxNode, params: Set<string>): boolean {
    if (node.type === "identifier" && params.has(node.text)) return true;
    for (const child of node.children) {
      if (this.referencesParam(child, params)) return true;
    }
    return false;
  }

  private findParamRef(node: Parser.SyntaxNode, params: Set<string>): string | null {
    if (node.type === "identifier" && params.has(node.text)) return node.text;
    for (const child of node.children) {
      const found = this.findParamRef(child, params);
      if (found) return found;
    }
    return null;
  }

  private hasGuard(node: Parser.SyntaxNode, patterns: string[], pathConditions: string[]): boolean {
    if (pathConditions.length === 0) return false;
    const joined = pathConditions.join(" ");
    for (const p of patterns) {
      if (joined.includes(p)) return true;
    }
    const identifiers = this.collectIdentifiers(node);
    for (const id of identifiers) {
      for (const cond of pathConditions) {
        if (cond.includes(id) && (
          cond.includes(">") || cond.includes("<") ||
          cond.includes("!==") || cond.includes("!=") ||
          cond.includes("==="))) {
          return true;
        }
      }
    }
    return false;
  }

  private collectIdentifiers(node: Parser.SyntaxNode): string[] {
    const ids: string[] = [];
    if (node.type === "identifier") ids.push(node.text);
    for (const child of node.children) ids.push(...this.collectIdentifiers(child));
    return ids;
  }

  private extractVarName(node: Parser.SyntaxNode | null): string {
    if (!node) return "expr";
    if (node.type === "identifier") return node.text;
    if (node.type === "member_expression") {
      const obj = node.childForFieldName("object");
      const prop = node.childForFieldName("property");
      return `${obj?.text || "obj"}_${prop?.text || "prop"}`;
    }
    return `expr_${node.startPosition.row}_${node.startPosition.column}`;
  }

  private isInsideTryCatch(node: Parser.SyntaxNode, fnNode: Parser.SyntaxNode): boolean {
    let current: Parser.SyntaxNode | null = node.parent;
    while (current && current.id !== fnNode.id) {
      if (current.type === "try_statement") {
        const tryBody = current.childForFieldName("body");
        if (tryBody && this.isDescendant(node, tryBody)) return true;
      }
      current = current.parent;
    }
    return false;
  }

  private isDescendant(node: Parser.SyntaxNode, ancestor: Parser.SyntaxNode): boolean {
    let current: Parser.SyntaxNode | null = node;
    while (current) {
      if (current.id === ancestor.id) return true;
      current = current.parent;
    }
    return false;
  }

  private findModifiedVars(node: Parser.SyntaxNode): string[] {
    const vars: string[] = [];
    const visit = (n: Parser.SyntaxNode): void => {
      if (n.type === "assignment_expression" || n.type === "augmented_assignment_expression") {
        const left = n.childForFieldName("left");
        if (left?.type === "identifier") vars.push(left.text);
      }
      for (const child of n.children) visit(child);
    };
    visit(node);
    return [...new Set(vars)];
  }

  private findAccumulator(body: Parser.SyntaxNode): string | null {
    let found: string | null = null;
    const visit = (node: Parser.SyntaxNode): void => {
      if (node.type === "augmented_assignment_expression" || node.type === "update_expression") {
        const left = node.childForFieldName("left") || node.firstNamedChild;
        if (left?.type === "identifier") found = left.text;
      }
      for (const child of node.children) visit(child);
    };
    visit(body);
    return found;
  }

  private bodyModifiesConditionVars(condition: Parser.SyntaxNode, body: Parser.SyntaxNode): boolean {
    const condVars = new Set<string>();
    const extractIds = (node: Parser.SyntaxNode): void => {
      if (node.type === "identifier") condVars.add(node.text);
      for (const child of node.children) extractIds(child);
    };
    extractIds(condition);

    let modifies = false;
    const visit = (node: Parser.SyntaxNode): void => {
      if (node.type === "assignment_expression" || node.type === "augmented_assignment_expression") {
        const left = node.childForFieldName("left");
        if (left?.type === "identifier" && condVars.has(left.text)) modifies = true;
      }
      for (const child of node.children) visit(child);
    };
    visit(body);
    return modifies;
  }

  private extractPathConditions(node: Parser.SyntaxNode, fnNode: Parser.SyntaxNode): string[] {
    const conditions: string[] = [];

    // Wrapping conditions: if-statements that contain this node
    let current: Parser.SyntaxNode | null = node.parent;
    while (current && current.id !== fnNode.id) {
      if (current.parent?.type === "if_statement") {
        const ifStmt = current.parent;
        const condition = ifStmt.childForFieldName("condition");
        const consequence = ifStmt.childForFieldName("consequence");
        const alternative = ifStmt.childForFieldName("alternative");
        if (condition) {
          if (consequence && this.isDescendant(node, consequence)) {
            conditions.unshift(condition.text);
          } else if (alternative && this.isDescendant(node, alternative)) {
            conditions.unshift(`!(${condition.text})`);
          }
        }
      }
      current = current.parent;
    }

    // Preceding early-return guards: scan enclosing blocks up to the
    // outer function. Include ALL enclosing blocks — both the nearest
    // callback scope and the outer function scope.
    const blocks: Parser.SyntaxNode[] = [];
    current = node.parent;
    while (current && current.id !== fnNode.id) {
      if (current.type === "statement_block") blocks.push(current);
      current = current.parent;
    }
    const fnBody = fnNode.childForFieldName("body");
    if (fnBody) blocks.push(fnBody);

    const nodeRow = node.startPosition.row;
    for (const block of blocks) {
      for (const stmt of block.namedChildren) {
        if (stmt.startPosition.row >= nodeRow) break;
        if (stmt.type === "if_statement") {
          const condition = stmt.childForFieldName("condition");
          const consequence = stmt.childForFieldName("consequence");
          if (!condition || !consequence) continue;
          if (this.blockReturnsOrThrows(consequence)) {
            conditions.unshift(`!(${condition.text})`);
          }
        }
      }
    }

    // Ternary conditions: if the node is inside a ternary's consequence,
    // the ternary condition is a path condition.
    current = node.parent;
    while (current && current.id !== fnNode.id) {
      if (current.type === "ternary_expression") {
        const cond = current.childForFieldName("condition");
        const consequence = current.childForFieldName("consequence");
        const alternative = current.childForFieldName("alternative");
        if (cond) {
          if (consequence && this.isDescendant(node, consequence)) {
            conditions.unshift(cond.text);
          } else if (alternative && this.isDescendant(node, alternative)) {
            conditions.unshift(`!(${cond.text})`);
          }
        }
      }
      current = current.parent;
    }

    // Loop condition: if the node is inside a for/while loop,
    // the loop condition is an implicit precondition
    current = node.parent;
    while (current && current.id !== fnNode.id) {
      if (current.type === "for_statement" || current.type === "while_statement") {
        const loopCond = current.childForFieldName("condition");
        if (loopCond) conditions.unshift(loopCond.text);
      }
      current = current.parent;
    }

    return conditions;
  }

  private isMapGetAfterSetOrHas(expr: Parser.SyntaxNode, node: Parser.SyntaxNode, fnNode: Parser.SyntaxNode): boolean {
    if (expr.type !== "call_expression") return false;
    const fn = expr.childForFieldName("function");
    if (!fn || fn.type !== "member_expression") return false;
    const method = fn.childForFieldName("property")?.text;
    if (method !== "get") return false;

    const mapObj = fn.childForFieldName("object")?.text;
    if (!mapObj) return false;

    const body = fnNode.childForFieldName("body");
    if (!body) return false;

    const nodeRow = node.startPosition.row;
    let foundSetOrHas = false;

    const visit = (n: Parser.SyntaxNode): void => {
      if (n.startPosition.row >= nodeRow) return;
      if (n.type === "call_expression") {
        const f = n.childForFieldName("function");
        if (f?.type === "member_expression") {
          const obj = f.childForFieldName("object")?.text;
          const m = f.childForFieldName("property")?.text;
          if (obj === mapObj && (m === "set" || m === "has")) foundSetOrHas = true;
        }
      }
      for (const child of n.children) visit(child);
    };

    visit(body);
    return foundSetOrHas;
  }

  private isWrappedByTruthyCheck(expr: Parser.SyntaxNode, pathConditions: string[]): boolean {
    const text = expr.text.replace(/!$/, "").trim();
    if (!text) return false;

    const base = text.split("[")[0]!.split(".")[0]!.trim();

    for (const cond of pathConditions) {
      if (cond === base) return true;
      if (cond === text) return true;
      if (cond.includes(base + " &&") || cond.includes("&& " + base)) return true;
      if (cond.includes(text + " &&") || cond.includes("&& " + text)) return true;
      if (cond.includes(base + "[") && cond.includes("]")) return true;
    }

    return false;
  }

  private callersAlwaysCatch(fnNode: Parser.SyntaxNode): boolean {
    const fnName = this.getFunctionName(fnNode);
    if (!fnName) return false;

    let root: Parser.SyntaxNode = fnNode;
    while (root.parent) root = root.parent;

    const callSites: Parser.SyntaxNode[] = [];
    const findCalls = (node: Parser.SyntaxNode): void => {
      if (node.type === "call_expression") {
        const fn = node.childForFieldName("function");
        if (fn) {
          const name = fn.type === "member_expression"
            ? fn.childForFieldName("property")?.text
            : fn.text;
          if (name === fnName && node.startPosition.row !== fnNode.startPosition.row) {
            callSites.push(node);
          }
        }
      }
      for (const child of node.children) findCalls(child);
    };
    findCalls(root);

    if (callSites.length === 0) return false;

    return callSites.every((site) => {
      let current: Parser.SyntaxNode | null = site.parent;
      while (current) {
        if (current.type === "try_statement") {
          const tryBody = current.childForFieldName("body");
          if (tryBody && this.isDescendant(site, tryBody)) return true;
        }
        if (current.type === "function_declaration" || current.type === "method_definition" ||
            current.type === "arrow_function" || current.type === "function_expression") break;
        current = current.parent;
      }
      return false;
    });
  }

  private getFunctionName(fnNode: Parser.SyntaxNode): string | null {
    const nameNode = fnNode.childForFieldName("name");
    if (nameNode) return nameNode.text;
    if (fnNode.parent?.type === "variable_declarator") {
      const varName = fnNode.parent.childForFieldName("name");
      if (varName) return varName.text;
    }
    return null;
  }

  private isBooleanExpression(node: Parser.SyntaxNode): boolean {
    if (node.type === "call_expression") {
      const fn = node.childForFieldName("function");
      if (fn?.type === "member_expression") {
        const method = fn.childForFieldName("property")?.text;
        if (method === "includes" || method === "has" || method === "startsWith" ||
            method === "endsWith" || method === "test" || method === "some" ||
            method === "every") return true;
      }
    }
    if (node.type === "binary_expression") {
      for (const child of node.children) {
        if (["===", "!==", "==", "!=", ">", "<", ">=", "<=", "instanceof", "in"].includes(child.type)) return true;
      }
    }
    if (node.type === "unary_expression") {
      const op = node.children.find((c) => c.type === "!");
      if (op) return true;
    }
    return false;
  }

  private resultUsedWithOptionalChaining(callNode: Parser.SyntaxNode): boolean {
    const parent = callNode.parent;
    if (!parent) return false;
    if (parent.type === "variable_declarator") {
      const varName = parent.childForFieldName("name")?.text;
      if (!varName) return false;
      const grandparent = parent.parent?.parent;
      if (!grandparent) return false;
      const siblings = grandparent.namedChildren;
      const declIdx = siblings.findIndex((s) => this.isDescendant(parent, s));
      for (let i = declIdx + 1; i < siblings.length; i++) {
        const text = siblings[i]!.text;
        if (text.includes(varName + "?.")) return true;
      }
    }
    return false;
  }

  private isValidationGuard(node: Parser.SyntaxNode, fnNode: Parser.SyntaxNode): boolean {
    let current: Parser.SyntaxNode | null = node.parent;
    while (current && current.id !== fnNode.id) {
      if (current.type === "if_statement") {
        const alternative = current.childForFieldName("alternative");
        if (!alternative) return true;
      }
      current = current.parent;
    }

    const body = fnNode.childForFieldName("body");
    if (body) {
      const stmts = body.namedChildren;
      const last = stmts[stmts.length - 1];
      if (last && this.isDescendant(node, last)) return true;
    }

    return false;
  }

  private isInsideCatchHandler(node: Parser.SyntaxNode, fnNode: Parser.SyntaxNode): boolean {
    let current: Parser.SyntaxNode | null = node.parent;
    while (current && current.id !== fnNode.id) {
      if (current.type === "catch_clause") return true;
      current = current.parent;
    }
    return false;
  }

  private findNearestFunction(node: Parser.SyntaxNode): Parser.SyntaxNode | null {
    let current: Parser.SyntaxNode | null = node.parent;
    while (current) {
      if (current.type === "function_declaration" || current.type === "method_definition" ||
          current.type === "arrow_function" || current.type === "function_expression") {
        return current;
      }
      current = current.parent;
    }
    return null;
  }

  private isIntentionalCleanup(tryBody: Parser.SyntaxNode): boolean {
    const stmts = tryBody.namedChildren;
    if (stmts.length !== 1) return false;
    const stmt = stmts[0]!;
    const expr = stmt.type === "expression_statement" ? stmt.firstNamedChild : stmt;
    if (!expr || expr.type !== "call_expression") return false;
    const fn = expr.childForFieldName("function");
    if (!fn) return false;
    const name = fn.type === "member_expression"
      ? fn.childForFieldName("property")?.text
      : fn.text;
    const cleanupFns = new Set(["unlinkSync", "unlink", "close", "closeSync", "abort", "destroy", "disconnect", "release", "dispose", "removeSync"]);
    return cleanupFns.has(name || "");
  }

  private isSplitIndexZero(expr: Parser.SyntaxNode): boolean {
    if (expr.type !== "subscript_expression") return false;
    const obj = expr.childForFieldName("object");
    const index = expr.childForFieldName("index");
    if (!obj || !index) return false;
    if (index.type !== "number" || index.text !== "0") return false;
    if (obj.type === "call_expression") {
      const fn = obj.childForFieldName("function");
      if (fn?.type === "member_expression") {
        const method = fn.childForFieldName("property")?.text;
        if (method === "split") return true;
      }
    }
    return false;
  }

  private resultIsCheckedAfterCall(callNode: Parser.SyntaxNode, fnNode: Parser.SyntaxNode): boolean {
    let assignedVar: string | null = null;
    const parent = callNode.parent;
    if (parent?.type === "variable_declarator") {
      const name = parent.childForFieldName("name");
      if (name) assignedVar = name.text;
    }
    if (parent?.type === "assignment_expression") {
      const left = parent.childForFieldName("left");
      if (left?.type === "identifier") assignedVar = left.text;
    }
    if (!assignedVar) return false;

    const body = fnNode.childForFieldName("body");
    if (!body) return false;

    const callRow = callNode.startPosition.row;
    let found = false;

    const visit = (node: Parser.SyntaxNode): void => {
      if (node.startPosition.row <= callRow) { for (const c of node.children) visit(c); return; }
      if (node.type === "if_statement") {
        const condition = node.childForFieldName("condition");
        if (condition && condition.text.includes(assignedVar!)) found = true;
      }
      for (const c of node.children) visit(c);
    };
    visit(body);
    return found;
  }

  private commandIsStaticString(callNode: Parser.SyntaxNode): boolean {
    const args = callNode.childForFieldName("arguments");
    if (!args || args.namedChildren.length === 0) return false;
    const firstArg = args.namedChildren[0]!;
    if (firstArg.type === "string") return true;
    if (firstArg.type === "template_string") {
      return firstArg.namedChildren.every((c) => c.type !== "template_substitution");
    }
    return false;
  }

  private isGuardedByLengthCheck(expr: Parser.SyntaxNode, pathConditions: string[]): boolean {
    let arrayName: string | null = null;

    if (expr.type === "subscript_expression") {
      const obj = expr.childForFieldName("object");
      if (obj) arrayName = obj.text;
    } else if (expr.type === "member_expression") {
      const obj = expr.childForFieldName("object");
      if (obj?.type === "subscript_expression") {
        const arrObj = obj.childForFieldName("object");
        if (arrObj) arrayName = arrObj.text;
      } else if (obj) {
        arrayName = obj.text;
      }
    }

    if (!arrayName) {
      const text = expr.text.split("[")[0]!.split(".")[0]!.split("!")[0]!.trim();
      if (text) arrayName = text;
    }

    if (!arrayName) return false;

    for (const cond of pathConditions) {
      if (cond.includes(arrayName) && cond.includes(".length")) return true;
      if (cond.includes(arrayName) && (cond.includes("> 0") || cond.includes(">= 1") || cond.includes(">= 2") || cond.includes("< 2"))) return true;
    }

    return false;
  }

  private isGuardedByNullCheckInConditions(expr: Parser.SyntaxNode, pathConditions: string[]): boolean {
    const text = expr.text.split("!")[0]!.split("[")[0]!.split(".")[0]!.trim();
    if (!text) return false;

    for (const cond of pathConditions) {
      if (cond.includes(text) && (
        cond.includes("!== null") || cond.includes("!= null") ||
        cond.includes("!== undefined") || cond.includes("!= undefined") ||
        cond.includes("!(!" + text + ")")
      )) return true;

      if (cond.startsWith("!(") && cond.includes(text)) {
        if (cond.includes("=== null") || cond.includes("=== undefined") ||
            cond.includes("== null") || cond.includes("== undefined")) return true;
      }

      if (cond === text || cond === `!(!${text})`) return true;
    }

    return false;
  }

  private paramOnlyInStdin(callNode: Parser.SyntaxNode, paramNames: Set<string>): boolean {
    const args = callNode.childForFieldName("arguments");
    if (!args || args.namedChildren.length === 0) return false;

    const firstArg = args.namedChildren[0];
    if (firstArg && this.referencesParam(firstArg, paramNames)) return false;

    for (let i = 1; i < args.namedChildren.length; i++) {
      const arg = args.namedChildren[i]!;
      if (arg.type === "object") {
        for (const prop of arg.namedChildren) {
          if (prop.type === "pair") {
            const key = prop.childForFieldName("key");
            const value = prop.childForFieldName("value");
            if (key?.text === "input" && value && this.referencesParam(value, paramNames)) {
              return true;
            }
          }
        }
      }
    }

    return false;
  }

  private isArrayIndexGuardedByLoop(expr: Parser.SyntaxNode, pathConditions: string[]): boolean {
    if (expr.type !== "subscript_expression") return false;

    const obj = expr.childForFieldName("object");
    const index = expr.childForFieldName("index");
    if (!obj || !index) return false;

    const objName = obj.text;
    const indexName = index.type === "identifier" ? index.text : null;
    if (!indexName) return false;

    for (const cond of pathConditions) {
      if (cond.includes(indexName) && (cond.includes(objName + ".length") || cond.includes(".length"))) {
        return true;
      }
    }

    return false;
  }

  private extractBindings(
    node: Parser.SyntaxNode,
    vars: Record<string, string>,
    smt2: string,
    line: number
  ): SmtBinding[] {
    // Build map of smt_constant -> sort from declare-const lines in the emitted smt2.
    const declareMap = new Map<string, string>();
    const declareRe = /\(declare-const\s+(\S+)\s+([A-Za-z][A-Za-z0-9]*)\)/g;
    let m: RegExpExecArray | null;
    while ((m = declareRe.exec(smt2)) !== null) {
      declareMap.set(m[1]!, m[2]!);
    }

    if (declareMap.size === 0) return [];

    // Build a lookup from safeName -> AST child node for binary_expression vars.
    // Keys produced by matchPattern for binary_expression: left, right, numerator, denominator, param.
    // numerator/denominator alias left/right, param aliases findParamRef result.
    // We resolve each to an AST child node (or null = use match node as fallback).
    const nodeForKey: Record<string, Parser.SyntaxNode | null> = {};
    if (node.type === "binary_expression") {
      const leftNode = node.childForFieldName("left") ?? null;
      const rightNode = node.childForFieldName("right") ?? null;
      nodeForKey["left"] = leftNode;
      nodeForKey["right"] = rightNode;
      nodeForKey["numerator"] = leftNode;
      nodeForKey["denominator"] = rightNode;
      // param: find the first identifier that is a param reference
      nodeForKey["param"] = null; // fallback to match node
    }

    const bindings: SmtBinding[] = [];
    const boundConstants = new Set<string>();

    // Process in a stable order: field-specific names first (numerator/denominator),
    // then generics (left/right/param), so dedup keeps the more meaningful entry.
    const orderedKeys = ["numerator", "denominator", "left", "right", "param"];
    const otherKeys = Object.keys(vars).filter(
      (k) => !k.startsWith("_") && !orderedKeys.includes(k)
    );
    const keysToProcess = [...orderedKeys, ...otherKeys].filter((k) => k in vars);

    for (const key of keysToProcess) {
      if (key.startsWith("_")) continue;
      const rawValue = vars[key];
      if (rawValue === undefined) continue;
      const safeName = rawValue.replace(/[^a-zA-Z0-9_]/g, "_").slice(0, 30) || key;
      if (!declareMap.has(safeName)) continue;
      if (boundConstants.has(safeName)) continue; // deduplicate
      boundConstants.add(safeName);

      const sort = declareMap.get(safeName)!;
      const childNode = nodeForKey[key] ?? null;
      const sourceNode = childNode ?? node;

      bindings.push({
        smt_constant: safeName,
        source_line: sourceNode.startPosition.row + 1,
        source_expr: sourceNode.text.slice(0, 80),
        sort,
      });
    }

    // Emit abstract bindings for synthetic constants (name_<line>) not covered above.
    const syntheticPattern = new RegExp(`^[a-zA-Z_][a-zA-Z0-9_]*_${line}$`);
    for (const [smtName, sort] of declareMap.entries()) {
      if (boundConstants.has(smtName)) continue;
      if (syntheticPattern.test(smtName)) {
        bindings.push({
          smt_constant: smtName,
          source_line: 0,
          source_expr: "<abstract>",
          sort,
        });
      }
    }

    return bindings;
  }

  private blockReturnsOrThrows(block: Parser.SyntaxNode): boolean {
    if (block.type === "return_statement" || block.type === "throw_statement" || block.type === "continue_statement" || block.type === "break_statement") return true;
    for (const child of block.namedChildren) {
      if (child.type === "return_statement" || child.type === "throw_statement" || child.type === "continue_statement" || child.type === "break_statement") return true;
      if (child.type === "expression_statement") {
        const expr = child.firstNamedChild;
        if (expr?.type === "call_expression") {
          const fn = expr.childForFieldName("function");
          if (fn?.text === "process.exit") return true;
        }
      }
    }
    return false;
  }
}
