import { execSync } from "child_process";
import Parser from "tree-sitter";
import { Contract, ProvenProperty } from "../contracts";
import { Checker, CheckResult } from "./Checker";
import { parseFile } from "../parser";
import { createProvider } from "../llm";
import { judgeRuntimeOutcome } from "../judge";

interface ExtractedFn {
  fn: (...args: any[]) => any;
  paramNames: string[];
  source: string;
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
}

export class PropertyTestChecker implements Checker {
  readonly name = "property-test";

  private tsNodeLoaded = false;
  private fnCache = new Map<string, ExtractedFn | null>();
  private limit: number;
  private used = 0;
  lastRun: PropertyTestContext[] = [];

  constructor() {
    const raw = process.env.NEURALLOG_PROPERTY_TEST_LIMIT;
    this.limit = raw ? Math.max(1, parseInt(raw, 10) || 10) : 10;
  }

  check(contracts: Contract[]): CheckResult[] {
    if (process.env.NEURALLOG_PROPERTY_TEST !== "1") return [];

    if (!this.ensureTsNode()) return [];

    const results: CheckResult[] = [];
    this.used = 0;
    this.lastRun = [];

    for (const contract of contracts) {
      if (this.used >= this.limit) break;
      for (const proven of contract.proven) {
        if (this.used >= this.limit) break;
        const r = this.runOne(contract, proven);
        if (r) {
          results.push(r.result);
          this.lastRun.push(r);
          this.used++;
        }
      }
    }

    return results;
  }

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

  private runOne(contract: Contract, proven: ProvenProperty): PropertyTestContext | null {
    const model = this.extractModel(proven.smt2);
    if (!model || Object.keys(model).length === 0) return null;

    const extracted = this.loadFunction(contract.file, contract.function);
    if (!extracted) return null;

    const args: any[] = [];
    for (const name of extracted.paramNames) {
      if (!(name in model)) return null;
      args.push(model[name]);
    }
    if (args.length === 0) return null;

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

    const verdict: "proven" | "violation" | "error" =
      outcome.kind === "threw" ? "violation" : "proven";

    const result: CheckResult = {
      checker: this.name,
      description: `[${proven.principle || "?"}] ${contract.function}:${contract.line} — with ${argSummary} → ${outcomeSummary}`,
      sourceContract: `${contract.function}:${contract.line}`,
      smt2: proven.smt2,
      expected: "unsat",
      z3Result: "unsat",
      verdict,
      error:
        outcome.kind === "threw"
          ? `runtime disagreement: Z3 said unsat but function threw on Z3-model input`
          : undefined,
    };

    return {
      result,
      claim: proven.claim,
      smt2: proven.smt2,
      functionSource: extracted.source,
      inputsSummary: argSummary,
      outcome,
    };
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

    const model: Record<string, number | boolean> = {};
    const modelRegex = /\(define-fun\s+(\S+)\s+\(\)\s+(Int|Real|Bool)\s+([^\s)]+(?:\s+[\d.\-]+)?)\s*\)/g;
    let m;
    while ((m = modelRegex.exec(output)) !== null) {
      const [, name, sort, rawValue] = m;
      const parsed = this.parseValue(sort!, rawValue!);
      if (parsed !== undefined) model[name!] = parsed;
    }

    const negRegex = /\(define-fun\s+(\S+)\s+\(\)\s+(Int|Real)\s+\(-\s+([\d.]+)\)\s*\)/g;
    while ((m = negRegex.exec(output)) !== null) {
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

    const extracted = this.extractParamNamesAndSource(filePath, fnName);
    if (!extracted) {
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
      return null;
    }

    const fn = mod?.[fnName] || mod?.default?.[fnName];
    if (typeof fn !== "function") {
      this.fnCache.set(cacheKey, null);
      return null;
    }

    const result = { fn, paramNames: extracted.paramNames, source: extracted.source };
    this.fnCache.set(cacheKey, result);
    return result;
  }

  private extractParamNamesAndSource(
    filePath: string,
    fnName: string
  ): { paramNames: string[]; source: string } | null {
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
      return { paramNames: names, source: node.text };
    } catch {
      return null;
    }
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
