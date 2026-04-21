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

    const fn = this.resolveCallable(mod, fnName, info);
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
    info: FunctionInfo
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
        const instance = new cls(...args);
        return (cls.prototype[fnName] as Function).bind(instance);
      } catch {
        continue;
      }
    }
    return null;
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
