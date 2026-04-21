import { execSync } from "child_process";
import { isAbsolute, resolve as resolvePath } from "path";
import Parser from "tree-sitter";
import { Contract, ProvenProperty, Violation } from "../contracts";
import { Checker, CheckResult } from "./Checker";
import { parseFile } from "../parser";
import { createProvider } from "../llm";
import { judgeRuntimeOutcome } from "../judge";

interface ExtractedFn {
  fn: (...args: any[]) => any;
  paramNames: string[];
  source: string;
}

interface FunctionInfo {
  paramNames: string[];
  source: string;
  isStatic: boolean;
  className: string | null;
}

interface CtorParamInfo {
  name: string;
  type: string;
}

interface PropertyTestContext {
  result: CheckResult;
  claim: string;
  smt2: string;
  functionSource: string;
  inputsSummary: string;
  outcome:
    | { kind: "returned"; value: string }
    | { kind: "threw"; error: string };
  source: "proven" | "violation";
}

export class PropertyTestChecker implements Checker {
  readonly name = "property-test";

  private tsNodeLoaded = false;
  private fnCache = new Map<string, ExtractedFn | null>();
  private fileRequireFailures = new Set<string>();
  private ctorParamsCache = new Map<string, CtorParamInfo[] | null>();
  private ctorArgsCache = new Map<string, any[] | null>();
  private limit: number;
  private attempted = 0;
  lastRun: PropertyTestContext[] = [];

  constructor(private projectRoot: string = process.cwd()) {
    const raw = process.env.NEURALLOG_PROPERTY_TEST_LIMIT;
    this.limit = raw ? Math.max(1, parseInt(raw, 10) || 10) : 10;
  }

  check(contracts: Contract[]): CheckResult[] {
    if (process.env.NEURALLOG_PROPERTY_TEST !== "1") return [];

    if (!this.ensureTsNode()) return [];

    const results: CheckResult[] = [];
    this.attempted = 0;
    this.lastRun = [];
    this.skipReasons = [];

    for (const contract of contracts) {
      if (this.attempted >= this.limit) break;
      for (const proven of contract.proven) {
        if (this.attempted >= this.limit) break;
        this.attempted++;
        const r = this.runOne(contract, proven, "proven");
        if (r) {
          results.push(r.result);
          this.lastRun.push(r);
        }
      }
    }

    for (const contract of contracts) {
      if (this.attempted >= this.limit) break;
      for (const violation of contract.violations) {
        if (this.attempted >= this.limit) break;
        if (!violation.witness) continue;
        this.attempted++;
        const r = this.runOne(contract, violation, "violation");
        if (r) {
          results.push(r.result);
          this.lastRun.push(r);
        }
      }
    }

    console.log(`[property-test] attempted ${this.attempted}, ran ${this.lastRun.length}, skipped ${this.attempted - this.lastRun.length}`);
    if (this.skipReasons.length > 0) {
      const reasons = new Map<string, number>();
      for (const r of this.skipReasons) {
        const bucket = r.replace(/"[^"]+"/g, '"..."').replace(/\b\S+\.ts\b/g, "<file>").replace(/[A-Za-z_][A-Za-z0-9_]*:\s*\d+/g, "<key>");
        reasons.set(bucket, (reasons.get(bucket) || 0) + 1);
      }
      const sorted = [...reasons.entries()].sort((a, b) => b[1] - a[1]);
      for (const [reason, n] of sorted) {
        console.log(`[property-test]   skipped ${n}: ${reason}`);
      }
    }
    return results;
  }

  private skipReasons: string[] = [];

  private ensureTsNode(): boolean {
    if (this.tsNodeLoaded) return true;
    try {
      require("ts-node/register");
      this.tsNodeLoaded = true;
      return true;
    } catch {
      console.log(`[property-test] ts-node/register unavailable; skipping`);
      return false;
    }
  }

  private runOne(
    contract: Contract,
    prop: ProvenProperty | Violation,
    source: "proven" | "violation"
  ): PropertyTestContext | null {
    const absPath = this.resolvePath(contract.file);
    if (this.fileRequireFailures.has(absPath)) {
      this.skipReasons.push("require() previously failed for file");
      return null;
    }

    const extracted = this.loadFunction(absPath, contract.function);
    if (!extracted) {
      this.skipReasons.push(`could not load ${contract.function} from ${contract.file}`);
      return null;
    }

    const witness = (prop as Violation).witness;
    const model = source === "violation" && witness
      ? this.parseZ3Model(witness)
      : this.extractModel(prop.smt2);

    if (!model || Object.keys(model).length === 0) {
      this.skipReasons.push(source === "violation" ? "witness empty / unparseable" : "no Z3 model (preconditions unsat or malformed)");
      return null;
    }

    const args: any[] = [];
    for (const name of extracted.paramNames) {
      if (!(name in model)) {
        this.skipReasons.push(`SMT model missing param "${name}" for ${contract.function}`);
        return null;
      }
      args.push(model[name]);
    }
    if (args.length === 0) {
      this.skipReasons.push(`${contract.function} has no parameters`);
      return null;
    }

    let outcome: { kind: "returned"; value: string } | { kind: "threw"; error: string };
    try {
      const value = extracted.fn(...args);
      outcome = { kind: "returned", value: this.formatValue(value) };
    } catch (e: any) {
      outcome = { kind: "threw", error: String(e?.message || e).slice(0, 200) };
    }

    const argSummary = args
      .map((a, i) => `${extracted.paramNames[i]}=${this.formatValue(a)}`)
      .join(", ");
    const outcomeSummary =
      outcome.kind === "returned"
        ? `returned ${outcome.value}`
        : `threw: ${outcome.error}`;

    let verdict: "proven" | "violation" | "error";
    let error: string | undefined;
    if (source === "proven") {
      verdict = outcome.kind === "threw" ? "violation" : "proven";
      error = outcome.kind === "threw"
        ? "runtime disagreement: Z3 said unsat (proven) but function threw on Z3-model input"
        : undefined;
    } else {
      verdict = outcome.kind === "threw" ? "violation" : "proven";
      error = outcome.kind === "threw"
        ? "violation reproduced: function threw on the Z3 witness — bug confirmed"
        : "violation did not reproduce: function ran cleanly on the Z3 witness — possible false positive";
    }

    const prefix = source === "violation" ? "VIOLATION-REPLAY " : "";
    const result: CheckResult = {
      checker: this.name,
      description: `[${prop.principle || "?"}] ${prefix}${contract.function}:${contract.line} — with ${argSummary} → ${outcomeSummary}`,
      sourceContract: `${contract.function}:${contract.line}`,
      smt2: prop.smt2,
      expected: source === "violation" ? "sat" : "unsat",
      z3Result: source === "violation" ? "sat" : "unsat",
      verdict,
      error,
    };

    return {
      result,
      claim: prop.claim,
      smt2: prop.smt2,
      functionSource: extracted.source,
      inputsSummary: argSummary,
      outcome,
      source,
    };
  }

  private resolvePath(file: string): string {
    return isAbsolute(file) ? file : resolvePath(this.projectRoot, file);
  }

  async judgeResults(model?: string): Promise<{ judged: number; flipped: number; confirmed: number }> {
    if (process.env.NEURALLOG_PROPERTY_TEST_JUDGE !== "1") {
      return { judged: 0, flipped: 0, confirmed: 0 };
    }
    if (this.lastRun.length === 0) {
      return { judged: 0, flipped: 0, confirmed: 0 };
    }

    const provider = createProvider();
    const judgeModel = model || process.env.NEURALLOG_JUDGE_MODEL || "claude-haiku-4-5-20251001";

    let judged = 0, flipped = 0, confirmed = 0;

    for (const ctx of this.lastRun) {
      const verdict = await judgeRuntimeOutcome(
        {
          functionSource: ctx.functionSource,
          claim: ctx.claim,
          smt2: ctx.smt2,
          inputsSummary: ctx.inputsSummary,
          outcome: ctx.outcome,
        },
        provider,
        judgeModel
      );
      judged++;

      const note = `property-judge: ${verdict.note}`;
      if (!verdict.valid && ctx.result.verdict === "proven") {
        ctx.result.verdict = "violation";
        ctx.result.error = `encoding-inconsistent — ${verdict.note}`;
        flipped++;
      } else if (!verdict.valid && ctx.result.verdict === "violation") {
        ctx.result.error = ctx.result.error ? `${ctx.result.error}; ${note}` : note;
        confirmed++;
      } else if (verdict.valid && ctx.result.verdict === "violation") {
        ctx.result.verdict = "proven";
        ctx.result.error = `runtime threw but judge considered outcome consistent with claim: ${verdict.note}`;
        flipped++;
      } else {
        ctx.result.error = ctx.result.error ? `${ctx.result.error}; ${note}` : note;
      }
    }

    return { judged, flipped, confirmed };
  }

  private extractModel(smt2: string): Record<string, number | boolean> | null {
    const lines = smt2.split("\n");
    const assertIndices: number[] = [];
    for (let i = 0; i < lines.length; i++) {
      if (lines[i]!.trim().startsWith("(assert")) assertIndices.push(i);
    }
    if (assertIndices.length === 0) return null;

    const goalIdx = assertIndices[assertIndices.length - 1]!;
    const preSmt = lines
      .filter((_, i) => i !== goalIdx)
      .join("\n")
      .replace(/\(check-sat\)/, "(check-sat)\n(get-model)");

    let output: string;
    try {
      output = execSync("z3 -in -T:5", { input: preSmt, encoding: "utf-8", timeout: 6000 });
    } catch {
      return null;
    }

    if (!/\bsat\b/.test(output)) return null;
    return this.parseZ3Model(output);
  }

  private parseZ3Model(text: string): Record<string, number | boolean> {
    const model: Record<string, number | boolean> = {};
    const modelRegex = /\(define-fun\s+(\S+)\s+\(\)\s+(Int|Real|Bool)\s+([^\s)]+(?:\s+[\d.\-]+)?)\s*\)/g;
    let m;
    while ((m = modelRegex.exec(text)) !== null) {
      const [, name, sort, rawValue] = m;
      const parsed = this.parseValue(sort!, rawValue!);
      if (parsed !== undefined) model[name!] = parsed;
    }
    const negRegex = /\(define-fun\s+(\S+)\s+\(\)\s+(Int|Real)\s+\(-\s+([\d.]+)\)\s*\)/g;
    while ((m = negRegex.exec(text)) !== null) {
      const [, name, sort, rawValue] = m;
      const parsed = this.parseValue(sort!, `-${rawValue!}`);
      if (parsed !== undefined) model[name!] = parsed;
    }
    return model;
  }

  private parseValue(sort: string, raw: string): number | boolean | undefined {
    if (sort === "Int") {
      const n = parseInt(raw, 10);
      return Number.isFinite(n) ? n : undefined;
    }
    if (sort === "Real") {
      if (raw.includes("/")) {
        const [a, b] = raw.split("/").map((s) => parseFloat(s));
        return b !== 0 ? a! / b! : undefined;
      }
      const n = parseFloat(raw);
      return Number.isFinite(n) ? n : undefined;
    }
    if (sort === "Bool") {
      if (raw === "true") return true;
      if (raw === "false") return false;
    }
    return undefined;
  }

  private loadFunction(filePath: string, fnName: string): ExtractedFn | null {
    const cacheKey = `${filePath}::${fnName}`;
    if (this.fnCache.has(cacheKey)) return this.fnCache.get(cacheKey)!;

    const info = this.extractFunctionInfo(filePath, fnName);
    if (!info) {
      this.fnCache.set(cacheKey, null);
      return null;
    }

    let mod: any;
    try {
      delete require.cache[require.resolve(filePath)];
      mod = require(filePath);
    } catch (e: any) {
      console.log(`[property-test] require failed for ${filePath}: ${e?.message?.slice(0, 60) || "unknown"}`);
      this.fnCache.set(cacheKey, null);
      this.fileRequireFailures.add(filePath);
      return null;
    }

    const fn = this.resolveCallable(mod, fnName, info, filePath);
    if (fn) {
      const result = { fn, paramNames: info.paramNames, source: info.source };
      this.fnCache.set(cacheKey, result);
      return result;
    }

    this.fnCache.set(cacheKey, null);
    return null;
  }

  private resolveCallable(
    mod: any,
    fnName: string,
    info: FunctionInfo,
    filePath: string
  ): ((...args: any[]) => any) | null {
    if (!info.className) {
      const fn = mod?.[fnName] || mod?.default?.[fnName];
      return typeof fn === "function" ? fn : null;
    }

    const cls = mod?.[info.className] || mod?.default?.[info.className];
    if (typeof cls !== "function") return null;

    if (info.isStatic) {
      const fn = cls[fnName];
      return typeof fn === "function" ? fn.bind(cls) : null;
    }

    if (typeof cls.prototype?.[fnName] !== "function") return null;

    // 1) First-attempt path: tree-sitter-based constructor introspection.
    // Synthesize mock args from the declared ctor param types.
    const instance = this.instantiateFromCtorIntrospection(cls, info.className, filePath, 0);
    if (instance !== null) {
      if (process.env.NEURALLOG_PROPERTY_TEST_DEBUG === "1") {
        console.log(`[property-test] ctor-introspection succeeded for ${info.className}`);
      }
      return (cls.prototype[fnName] as Function).bind(instance);
    }
    if (process.env.NEURALLOG_PROPERTY_TEST_DEBUG === "1") {
      console.log(`[property-test] ctor-introspection failed for ${info.className}, trying fallback`);
    }

    // 2) Fallback: hardcoded ctor arg patterns (safety net for cases where
    // introspection fails or the type annotation is missing).
    const ctorAttempts: any[][] = [
      [],
      [this.projectRoot],
      [{}],
      [{ projectRoot: this.projectRoot }],
      [this.projectRoot, false],
      [{ projectRoot: this.projectRoot, verbose: false }],
    ];

    for (const args of ctorAttempts) {
      try {
        const fallbackInstance = new cls(...args);
        return (cls.prototype[fnName] as Function).bind(fallbackInstance);
      } catch {
        continue;
      }
    }
    return null;
  }

  /**
   * Try to instantiate `cls` by parsing its constructor's declared param types
   * (via tree-sitter) and synthesizing a mock value for each. Returns null if
   * parsing or instantiation fails. Caches per class+file so re-tests don't
   * re-parse.
   *
   * `depth` guards against recursive construction cycles when a ctor param is
   * itself a named class type.
   */
  private instantiateFromCtorIntrospection(
    cls: any,
    className: string,
    filePath: string,
    depth: number
  ): any | null {
    const cacheKey = `${filePath}::${className}`;
    let cached = this.ctorArgsCache.get(cacheKey);
    if (cached === undefined) {
      const params = this.extractConstructorParams(filePath, className);
      if (params === null) {
        // No ctor declaration found — try zero-arg construction.
        cached = [];
      } else {
        try {
          cached = params.map((p) => this.synthesizeMockValue(p.type, p.name, depth));
        } catch {
          cached = null;
        }
      }
      this.ctorArgsCache.set(cacheKey, cached);
    }
    if (cached === null) return null;
    try {
      return new cls(...cached);
    } catch {
      return null;
    }
  }

  /**
   * Parse the given source file and return the constructor's param list for
   * the named class. Returns null if no explicit constructor (caller then
   * tries zero-arg construction, which is correct for a class with no ctor).
   * Returns an empty array if the constructor exists but has no params.
   */
  private extractConstructorParams(
    filePath: string,
    className: string
  ): CtorParamInfo[] | null {
    const cacheKey = `${filePath}::${className}`;
    if (this.ctorParamsCache.has(cacheKey)) {
      return this.ctorParamsCache.get(cacheKey)!;
    }

    let result: CtorParamInfo[] | null = null;
    try {
      const source = require("fs").readFileSync(filePath, "utf-8");
      const tree = parseFile(source);

      let classNode: Parser.SyntaxNode | null = null;
      const visit = (node: Parser.SyntaxNode): void => {
        if (classNode) return;
        if (node.type === "class_declaration" || node.type === "class") {
          const nameNode = node.childForFieldName("name");
          if (nameNode?.text === className) {
            classNode = node;
            return;
          }
        }
        for (const child of node.children) visit(child);
      };
      visit(tree.rootNode);

      if (classNode) {
        const body = (classNode as Parser.SyntaxNode).childForFieldName("body");
        let ctorNode: Parser.SyntaxNode | null = null;
        if (body) {
          for (const child of body.namedChildren) {
            if (child.type === "method_definition") {
              const nameNode = child.childForFieldName("name");
              if (nameNode?.text === "constructor") {
                ctorNode = child;
                break;
              }
            }
          }
        }
        if (ctorNode) {
          const paramsNode = ctorNode.childForFieldName("parameters");
          const params: CtorParamInfo[] = [];
          if (paramsNode) {
            for (const child of paramsNode.namedChildren) {
              if (
                child.type === "required_parameter" ||
                child.type === "optional_parameter"
              ) {
                const nameNode =
                  child.childForFieldName("pattern") ||
                  child.childForFieldName("name");
                const typeNode = child.childForFieldName("type");
                params.push({
                  name: nameNode?.text || "?",
                  type: typeNode ? typeNode.text.replace(/^:\s*/, "").trim() : "unknown",
                });
              }
            }
          }
          result = params;
        }
      }
    } catch {
      result = null;
    }

    this.ctorParamsCache.set(cacheKey, result);
    return result;
  }

  /**
   * Synthesize a mock value matching `type`. Used to fabricate constructor
   * arguments from declared TS types. Best-effort; may throw for recursion
   * overflow so the caller can fall back to hardcoded patterns.
   */
  private synthesizeMockValue(type: string, paramName: string, depth: number): any {
    const t = type.trim();
    if (!t || t === "unknown" || t === "any") return undefined;

    // Optional / nullable / union — strip undefined/null and pick first branch.
    const noParens = t.replace(/^\((.*)\)$/s, "$1");
    if (noParens.includes("|")) {
      const branches = this.splitTopLevel(noParens, "|").map((b) => b.trim());
      const nonNullish = branches.filter(
        (b) => b !== "undefined" && b !== "null" && b !== "void"
      );
      if (nonNullish.length === 0) return undefined;
      return this.synthesizeMockValue(nonNullish[0]!, paramName, depth);
    }

    // Primitives
    if (t === "string") {
      const lower = paramName.toLowerCase();
      if (
        lower.includes("path") ||
        lower.includes("root") ||
        lower.includes("dir") ||
        lower.includes("file")
      ) {
        return this.projectRoot;
      }
      return "";
    }
    if (t === "number") return 0;
    if (t === "boolean") return false;
    if (t === "bigint") return BigInt(0);
    if (t === "void" || t === "null") return null;
    if (t === "undefined" || t === "never") return undefined;

    // Array<T> / T[] / readonly T[]
    const readonlyStripped = t.replace(/^readonly\s+/, "");
    if (/^\w.*\[\]$/.test(readonlyStripped) || /\]\s*\[\]$/.test(readonlyStripped)) {
      return [];
    }
    const arrayGeneric = readonlyStripped.match(/^Array\s*<([\s\S]*)>$/);
    if (arrayGeneric) return [];
    const readonlyArrayGeneric = readonlyStripped.match(/^ReadonlyArray\s*<([\s\S]*)>$/);
    if (readonlyArrayGeneric) return [];

    // Map<K,V> / ReadonlyMap<K,V>
    if (/^(Readonly)?Map\s*<[\s\S]*>$/.test(readonlyStripped)) return new Map();
    // Set<T> / ReadonlySet<T>
    if (/^(Readonly)?Set\s*<[\s\S]*>$/.test(readonlyStripped)) return new Set();
    // Common built-ins
    if (readonlyStripped === "Date") return new Date(0);
    if (readonlyStripped === "RegExp") return /(?:)/;
    if (/^Record\s*<[\s\S]*>$/.test(readonlyStripped)) return {};
    if (/^Promise\s*<[\s\S]*>$/.test(readonlyStripped)) return Promise.resolve(undefined);

    // Inline object type: { a: string; b: number }
    if (readonlyStripped.startsWith("{") && readonlyStripped.endsWith("}")) {
      return this.synthesizeInlineObject(readonlyStripped, depth);
    }

    // Tuple [A, B]
    if (readonlyStripped.startsWith("[") && readonlyStripped.endsWith("]")) {
      return [];
    }

    // Function type: (...) => ...  or  (a: X) => Y
    if (/=>/.test(readonlyStripped) && readonlyStripped.includes("(")) {
      return () => undefined;
    }

    // Named type — maybe a class we can recursively instantiate.
    // Only try this at depth 0 to avoid cycles.
    if (depth < 1) {
      const bareName = readonlyStripped.replace(/<[\s\S]*>$/, "").trim();
      if (/^[A-Z][\w$]*$/.test(bareName)) {
        const synthesized = this.tryInstantiateNamedType(bareName, depth + 1);
        if (synthesized !== undefined) return synthesized;
      }
    }

    // Fallback: empty object for any unresolved named type.
    return {};
  }

  /**
   * Parse an inline object type like `{ verbose: boolean; name: string }`
   * and return a matching object with mock values.
   */
  private synthesizeInlineObject(type: string, depth: number): Record<string, any> {
    const inner = type.slice(1, -1).trim();
    if (!inner) return {};
    const out: Record<string, any> = {};
    const members = this.splitTopLevel(inner, ";")
      .flatMap((m) => this.splitTopLevel(m, ","))
      .map((m) => m.trim())
      .filter(Boolean);
    for (const member of members) {
      // Skip signature members like `[key: string]: ...` or methods.
      if (member.startsWith("[")) continue;
      const colonIdx = this.findTopLevelColon(member);
      if (colonIdx < 0) continue;
      let key = member.slice(0, colonIdx).trim();
      // Strip readonly, ?, and surrounding quotes.
      key = key.replace(/^readonly\s+/, "");
      const optional = key.endsWith("?");
      if (optional) key = key.slice(0, -1).trim();
      key = key.replace(/^["']|["']$/g, "");
      if (!key || /[\s()]/.test(key)) continue;
      const valueType = member.slice(colonIdx + 1).trim();
      try {
        out[key] = this.synthesizeMockValue(valueType, key, depth);
      } catch {
        out[key] = undefined;
      }
    }
    return out;
  }

  /**
   * Best-effort instantiation of a named type (e.g. `ContractStore`) by trying
   * a small set of ctor arg patterns. Used recursively when a ctor param is
   * itself a named class.
   */
  private tryInstantiateNamedType(typeName: string, depth: number): any | undefined {
    if (depth > 1) return undefined;
    // We don't have a file path for an arbitrary named type; rely on the
    // hardcoded fallback list, which covers common projectRoot-style ctors.
    const candidates: any[][] = [
      [this.projectRoot],
      [],
      [{}],
      [{ projectRoot: this.projectRoot }],
    ];
    // Walk require.cache for any loaded module that exports this name.
    for (const mod of Object.values(require.cache)) {
      const exp: any = (mod as any)?.exports;
      if (!exp) continue;
      const cls = exp[typeName] ?? exp.default?.[typeName];
      if (typeof cls !== "function") continue;
      for (const args of candidates) {
        try {
          return new cls(...args);
        } catch {
          continue;
        }
      }
    }
    return undefined;
  }

  /** Split `input` on `sep` at top level only (ignore nested <>, (), [], {}). */
  private splitTopLevel(input: string, sep: string): string[] {
    const out: string[] = [];
    let depth = 0;
    let last = 0;
    for (let i = 0; i < input.length; i++) {
      const c = input[i];
      if (c === "<" || c === "(" || c === "[" || c === "{") depth++;
      else if (c === ">" || c === ")" || c === "]" || c === "}") depth--;
      else if (c === sep && depth === 0) {
        out.push(input.slice(last, i));
        last = i + 1;
      }
    }
    out.push(input.slice(last));
    return out;
  }

  /** Find the first top-level `:` in `input`, skipping nested brackets. */
  private findTopLevelColon(input: string): number {
    let depth = 0;
    for (let i = 0; i < input.length; i++) {
      const c = input[i];
      if (c === "<" || c === "(" || c === "[" || c === "{") depth++;
      else if (c === ">" || c === ")" || c === "]" || c === "}") depth--;
      else if (c === ":" && depth === 0) return i;
    }
    return -1;
  }

  private extractFunctionInfo(filePath: string, fnName: string): FunctionInfo | null {
    try {
      const source = require("fs").readFileSync(filePath, "utf-8");
      const tree = parseFile(source);

      let target: Parser.SyntaxNode | null = null;
      const visit = (node: Parser.SyntaxNode): void => {
        if (target) return;
        if (
          node.type === "function_declaration" ||
          node.type === "method_definition" ||
          node.type === "arrow_function" ||
          node.type === "function_expression"
        ) {
          const nameNode = node.childForFieldName("name");
          const name = nameNode?.text;
          if (name === fnName) target = node;
          else if (
            node.parent?.type === "variable_declarator" &&
            node.parent.childForFieldName("name")?.text === fnName
          ) {
            target = node;
          }
        }
        for (const child of node.children) visit(child);
      };
      visit(tree.rootNode);
      if (!target) return null;

      const node = target as Parser.SyntaxNode;
      const paramsNode = node.childForFieldName("parameters");
      if (!paramsNode) return null;

      const names: string[] = [];
      for (const child of paramsNode.namedChildren) {
        const patternNode = child.childForFieldName("pattern") || child.childForFieldName("name");
        if (patternNode?.type === "identifier") {
          names.push(patternNode.text);
        } else {
          return null;
        }
      }

      const isStatic = node.children.some((c) => c.text === "static" && c.type === "static");
      const className = this.findEnclosingClassName(node);

      return { paramNames: names, source: node.text, isStatic, className };
    } catch {
      return null;
    }
  }

  private findEnclosingClassName(node: Parser.SyntaxNode): string | null {
    let current: Parser.SyntaxNode | null = node.parent;
    while (current) {
      if (current.type === "class_declaration" || current.type === "class") {
        const nameNode = current.childForFieldName("name");
        if (nameNode) return nameNode.text;
      }
      current = current.parent;
    }
    return null;
  }

  private formatValue(v: any): string {
    if (v === undefined) return "undefined";
    if (v === null) return "null";
    if (typeof v === "string") return JSON.stringify(v).slice(0, 40);
    try {
      return JSON.stringify(v).slice(0, 40);
    } catch {
      return String(v).slice(0, 40);
    }
  }
}
