import { execSync } from "child_process";
import { isAbsolute, resolve as resolvePath } from "path";
import Parser from "tree-sitter";
import { Contract, ContractStore, ProvenProperty, Violation } from "../contracts";
import { Checker, CheckResult } from "./Checker";
import { parseFile } from "../parser";
import { createProvider } from "../llm";
import { judgeRuntimeOutcome } from "../judge";
import { synthesizeHarness, runHarness, HarnessCache, HarnessOutcome } from "../harness";
import { judgeHarnessCode } from "../judge";
import { JudgeCache } from "../judgeCache";
import { loadModuleWithPrivates, collectTransitiveSource } from "../moduleLoader";
import { LessonStore } from "../lessons";
import { readObservations } from "../runtime";
import { findTestsForFunction, formatForPrompt as formatTestOracleForPrompt, detectTestFramework, runTestsForReferences, summarizeTriangle } from "../testOracle";

interface ExtractedFn {
  fn: (...args: any[]) => any;
  paramNames: string[];
  paramTypes: string[];
  source: string;
}

interface FunctionInfo {
  paramNames: string[];
  paramTypes: string[];
  source: string;
  isStatic: boolean;
  className: string | null;
}

interface CtorParamInfo {
  name: string;
  type: string;
}

interface BoundaryRun {
  args: any[];
  outcome:
    | { kind: "returned"; value: string }
    | { kind: "threw"; error: string };
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
  contractKey: string;
  boundaryRuns: BoundaryRun[];
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
  private harnessCandidates: Array<{
    contract: Contract;
    prop: ProvenProperty | Violation;
    source: "proven" | "violation";
    skipReason: string;
  }> = [];
  harnessResults: CheckResult[] = [];

  constructor(private projectRoot: string = process.cwd()) {
    const raw = process.env.NEURALLOG_PROPERTY_TEST_LIMIT;
    this.limit = raw ? Math.max(1, parseInt(raw, 10) || 10) : 10;
  }

  check(contracts: Contract[], _callGraph: Map<string, string[]>): CheckResult[] {
    if (process.env.NEURALLOG_PROPERTY_TEST !== "1") return [];

    if (!this.ensureTsNode()) return [];

    const results: CheckResult[] = [];
    this.attempted = 0;
    this.lastRun = [];
    this.skipReasons = [];
    this.harnessCandidates = [];
    this.harnessResults = [];

    const sampleCount = Math.max(1, parseInt(process.env.NEURALLOG_Z3_SAMPLES || "1", 10) || 1);
    const useObservations = process.env.NEURALLOG_USE_RUNTIME_OBSERVATIONS === "1";

    for (const contract of contracts) {
      if (this.attempted >= this.limit) break;
      for (const proven of contract.proven) {
        if (this.attempted >= this.limit) break;

        const models: Array<Record<string, number | boolean> | null> = [];
        if (useObservations) {
          const obs = readObservations(contract.function, this.projectRoot);
          for (const o of obs.slice(0, sampleCount)) {
            const primitive: Record<string, number | boolean> = {};
            for (const [k, v] of Object.entries(o.values)) {
              if (typeof v === "number" || typeof v === "boolean") primitive[k] = v;
            }
            if (Object.keys(primitive).length > 0) models.push(primitive);
          }
        }
        if (models.length === 0) {
          if (sampleCount === 1) models.push(null);
          else for (const m of this.extractModels(proven.smt2, sampleCount)) models.push(m);
        }

        for (const modelOverride of models) {
          if (this.attempted >= this.limit) break;
          this.attempted++;
          const r = this.runOne(contract, proven, "proven", modelOverride || undefined);
          if (r) {
            results.push(r.result);
            this.lastRun.push(r);
          }
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
      require("ts-node").register({ transpileOnly: true, compilerOptions: { module: "commonjs" } });
      this.tsNodeLoaded = true;
      return true;
    } catch {
      try {
        require("ts-node/register/transpile-only");
        this.tsNodeLoaded = true;
        return true;
      } catch {
        console.log(`[property-test] ts-node unavailable; skipping`);
        return false;
      }
    }
  }

  private runOne(
    contract: Contract,
    prop: ProvenProperty | Violation,
    source: "proven" | "violation",
    modelOverride?: Record<string, number | boolean>
  ): PropertyTestContext | null {
    const absPath = this.resolvePath(contract.file);
    if (this.fileRequireFailures.has(absPath)) {
      this.skipReasons.push("require() previously failed for file");
      return null;
    }

    if (contract.function === "constructor") {
      this.skipReasons.push("constructor — property test needs new-invocation protocol, skipped");
      return null;
    }

    const extracted = this.loadFunction(absPath, contract.function);
    if (!extracted) {
      this.skipReasons.push(`could not load ${contract.function} from ${contract.file}`);
      return null;
    }

    const witness = (prop as Violation).witness;
    const model = modelOverride
      ? modelOverride
      : source === "violation" && witness
        ? this.parseZ3Model(witness)
        : this.extractModel(prop.smt2);

    if (!model || Object.keys(model).length === 0) {
      this.skipReasons.push(source === "violation" ? "witness empty / unparseable" : "no Z3 model (preconditions unsat or malformed)");
      return null;
    }

    if (this.isControlFlowModel(model, extracted.paramNames)) {
      this.skipReasons.push(`control-flow principle (guard/branch abstraction) — not runnable via property test for ${contract.function}`);
      return null;
    }

    const nonDet = this.detectsNonDeterminism(extracted.source);
    if (nonDet) {
      const reason = `${contract.function} references ${nonDet} (non-deterministic) — primitive path cannot stub; routed to harness synthesis`;
      this.skipReasons.push(reason);
      this.harnessCandidates.push({ contract, prop, source, skipReason: reason });
      return null;
    }

    const paramTypes = extracted.paramTypes || extracted.paramNames.map(() => "unknown");
    const args: any[] = [];
    for (let i = 0; i < extracted.paramNames.length; i++) {
      const name = extracted.paramNames[i]!;
      const tsType = paramTypes[i] || "unknown";
      const matched = this.matchParamToModel(name, model);
      if (matched === undefined) {
        const reason = `SMT model missing param "${name}" for ${contract.function}`;
        this.skipReasons.push(reason);
        this.harnessCandidates.push({ contract, prop, source, skipReason: reason });
        return null;
      }
      if (!this.isCompatibleWithTsType(tsType, matched)) {
        const reason = `SMT value ${typeof matched} for param "${name}" incompatible with declared type "${tsType}" in ${contract.function}`;
        this.skipReasons.push(reason);
        this.harnessCandidates.push({ contract, prop, source, skipReason: reason });
        return null;
      }
      args.push(matched);
    }
    if (args.length === 0) {
      this.skipReasons.push(`${contract.function} has no parameters`);
      return null;
    }

    const outcome = this.callFunction(extracted.fn, args);
    const boundaryRuns = this.runBoundarySamples(extracted.fn, extracted.paramNames, args, prop.smt2);

    const argSummary = args
      .map((a, i) => `${extracted.paramNames[i]}=${this.formatValue(a)}`)
      .join(", ");
    const outcomeSummary =
      outcome.kind === "returned"
        ? `returned ${outcome.value}`
        : `threw: ${outcome.error}`;

    const boundaryThrows = boundaryRuns.filter((b) => b.outcome.kind === "threw").length;
    const boundaryReturns = boundaryRuns.length - boundaryThrows;
    const boundarySummary = boundaryRuns.length === 0
      ? ""
      : boundaryThrows === 0
        ? ` [+${boundaryRuns.length} boundary cases all clean]`
        : boundaryThrows === boundaryRuns.length
          ? ` [+${boundaryRuns.length} boundary cases all threw]`
          : ` [+${boundaryRuns.length} boundary cases: ${boundaryReturns} clean, ${boundaryThrows} threw]`;

    let verdict: "proven" | "violation" | "error";
    let error: string | undefined;
    if (source === "proven") {
      const anyBoundaryThrew = boundaryThrows > 0;
      if (outcome.kind === "threw") {
        verdict = "violation";
        error = "runtime disagreement: Z3 said unsat (proven) but function threw on Z3-model input";
      } else if (anyBoundaryThrew) {
        verdict = "violation";
        error = `runtime disagreement on boundary input: function threw on ${boundaryThrows}/${boundaryRuns.length} boundary variants that still satisfied preconditions`;
      } else {
        verdict = "proven";
        error = undefined;
      }
    } else {
      if (outcome.kind === "threw") {
        verdict = "violation";
        error = "violation reproduced: function threw on the Z3 witness — bug confirmed";
      } else {
        verdict = "error";
        error = `violation did not reproduce on witness — function returned cleanly where Z3 predicted a fault; needs judge to decide if the claim is a false positive or if the runtime behaviour still contradicts the claim's semantics`;
      }
    }

    const prefix = source === "violation" ? "VIOLATION-REPLAY " : "";
    const result: CheckResult = {
      checker: this.name,
      description: `[${prop.principle || "?"}] ${prefix}${contract.function}:${contract.line} — with ${argSummary} → ${outcomeSummary}${boundarySummary}`,
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
      contractKey: contract.key,
      boundaryRuns,
    };
  }

  private callFunction(fn: (...args: any[]) => any, args: any[]): { kind: "returned"; value: string } | { kind: "threw"; error: string } {
    try {
      const value = fn(...args);
      return { kind: "returned", value: this.formatValue(value) };
    } catch (e: any) {
      return { kind: "threw", error: String(e?.message || e).slice(0, 200) };
    }
  }

  private runBoundarySamples(
    fn: (...args: any[]) => any,
    paramNames: string[],
    primaryArgs: any[],
    smt2: string
  ): BoundaryRun[] {
    const runs: BoundaryRun[] = [];
    const candidates = [0, 1, -1];

    for (let i = 0; i < primaryArgs.length; i++) {
      const primary = primaryArgs[i];
      if (typeof primary !== "number") continue;

      for (const cand of candidates) {
        if (cand === primary) continue;
        const variantArgs = primaryArgs.slice();
        variantArgs[i] = cand;

        if (!this.isConsistentWithPreconditions(smt2, paramNames, variantArgs)) continue;

        const outcome = this.callFunction(fn, variantArgs);
        runs.push({ args: variantArgs, outcome });

        if (runs.length >= 8) return runs;
      }
    }
    return runs;
  }

  private isConsistentWithPreconditions(smt2: string, paramNames: string[], args: any[]): boolean {
    const lines = smt2.split("\n");
    const assertIndices: number[] = [];
    for (let i = 0; i < lines.length; i++) {
      if (lines[i]!.trim().startsWith("(assert")) assertIndices.push(i);
    }
    if (assertIndices.length === 0) return true;

    const goalIdx = assertIndices[assertIndices.length - 1]!;
    const pinnings: string[] = [];
    for (let i = 0; i < paramNames.length; i++) {
      const v = args[i];
      if (typeof v === "number") {
        const lit = v < 0 ? `(- ${-v})` : String(v);
        pinnings.push(`(assert (= ${paramNames[i]} ${lit}))`);
      } else if (typeof v === "boolean") {
        pinnings.push(`(assert (= ${paramNames[i]} ${v}))`);
      }
    }
    const block = lines
      .filter((_, i) => i !== goalIdx)
      .concat(pinnings)
      .join("\n");

    try {
      const output = execSync("z3 -in -T:3", { input: block, encoding: "utf-8", timeout: 4000 });
      return /\bsat\b/.test(output) && !/\bunsat\b/.test(output);
    } catch {
      return false;
    }
  }

  private resolvePath(file: string): string {
    return isAbsolute(file) ? file : resolvePath(this.projectRoot, file);
  }

  private async parallelMap<T, R>(items: T[], concurrency: number, fn: (item: T) => Promise<R>): Promise<R[]> {
    const results: R[] = new Array(items.length);
    let idx = 0;
    const workers = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
      while (true) {
        const i = idx++;
        if (i >= items.length) return;
        results[i] = await fn(items[i]!);
      }
    });
    await Promise.all(workers);
    return results;
  }

  private detectsNonDeterminism(source: string): string | null {
    const patterns: [RegExp, string][] = [
      [/\bMath\.random\b/, "Math.random"],
      [/\bDate\.now\b/, "Date.now"],
      [/\bperformance\.now\b/, "performance.now"],
      [/\bnew\s+Date\s*\(\s*\)/, "new Date()"],
      [/\bprocess\.hrtime\b/, "process.hrtime"],
      [/\bcrypto\.(?:randomUUID|randomBytes|getRandomValues)\b/, "crypto random"],
    ];
    for (const [re, name] of patterns) {
      if (re.test(source)) return name;
    }
    return null;
  }

  private isCompatibleWithTsType(tsType: string, value: unknown): boolean {
    if (!tsType || tsType === "unknown" || tsType === "any") return true;
    const t = tsType.trim();
    const jsType = typeof value;

    if (t.includes("|")) {
      return t.split("|").some((part) => this.isCompatibleWithTsType(part.trim(), value));
    }

    if (t === "undefined") return jsType === "undefined";
    if (t === "null") return value === null;

    if (jsType === "number") {
      if (/^(number|Number)$/.test(t)) return true;
      if (/^-?\d+(\.\d+)?$/.test(t)) return true;
      if (/^bigint$/i.test(t)) return false;
    }
    if (jsType === "boolean") {
      if (/^(boolean|Boolean|true|false)$/.test(t)) return true;
    }
    if (jsType === "string") {
      if (/^(string|String)$/.test(t)) return true;
      if (/^".*"$/.test(t) || /^'.*'$/.test(t)) return true;
    }

    return false;
  }

  private isControlFlowModel(
    model: Record<string, number | boolean>,
    paramNames: string[]
  ): boolean {
    const keys = Object.keys(model);
    if (keys.length === 0) return false;
    const controlPrefixSet = new Set([
      "guard_condition", "guard_returns", "guard_value",
      "code_after_reached", "code_after_reached_flag",
      "branch_reached", "branch_taken", "branch_active",
      "result_consequent", "result_alternate", "result_true", "result_false",
      "cond", "condition_true", "condition_false",
    ]);
    const paramSet = new Set(paramNames);
    const looksControl = (k: string): boolean => {
      if (controlPrefixSet.has(k)) return true;
      if (/^(guard|branch|result|cond|code_after|path)[_-]/.test(k)) return true;
      return false;
    };
    if (!keys.every(looksControl)) return false;
    return !keys.some((k) => paramSet.has(k));
  }

  private matchParamToModel(
    param: string,
    model: Record<string, number | boolean>
  ): number | boolean | undefined {
    if (param in model) return model[param];

    const target = param.toLowerCase().replace(/_/g, "");
    const keys = Object.keys(model);

    const exactNorm = keys.find((k) => k.toLowerCase().replace(/_/g, "") === target);
    if (exactNorm) return model[exactNorm];

    const suffixes = ["_condition", "_value", "_returns", "_guard", "_consequent", "_alternate", "_old", "_new", "_var"];
    for (const k of keys) {
      for (const sfx of suffixes) {
        if (k.toLowerCase().endsWith(sfx)) {
          const stripped = k.slice(0, -sfx.length).toLowerCase().replace(/_/g, "");
          if (stripped === target) return model[k];
        }
      }
    }

    const substringMatch = keys.find(
      (k) => {
        const n = k.toLowerCase().replace(/_/g, "");
        return n.includes(target) || target.includes(n);
      }
    );
    if (substringMatch) return model[substringMatch];

    return undefined;
  }

  async judgeResults(model?: string): Promise<{ judged: number; flipped: number; confirmed: number; cacheHits: number }> {
    if (process.env.NEURALLOG_PROPERTY_TEST_JUDGE !== "1") {
      return { judged: 0, flipped: 0, confirmed: 0, cacheHits: 0 };
    }
    if (this.lastRun.length === 0) {
      return { judged: 0, flipped: 0, confirmed: 0, cacheHits: 0 };
    }

    const provider = createProvider();
    const judgeModel = model || process.env.NEURALLOG_JUDGE_MODEL || "claude-haiku-4-5-20251001";
    const judgeCache = new JudgeCache(this.projectRoot);

    let judged = 0, flipped = 0, confirmed = 0;
    let cacheHits = 0;
    const store = new ContractStore(this.projectRoot);
    const touched = new Set<string>();

    for (const ctx of this.lastRun) {
      const outcomeSig = ctx.outcome.kind === "returned"
        ? `returned:${ctx.outcome.value}`
        : `threw:${ctx.outcome.error}`;
      const cacheParts = [ctx.smt2, ctx.functionSource, ctx.inputsSummary, outcomeSig, ctx.source];

      const cached = judgeCache.get(...cacheParts);
      let verdict: { valid: boolean; note: string };
      if (cached) {
        verdict = { valid: cached.valid, note: cached.note };
        cacheHits++;
      } else {
        verdict = await judgeRuntimeOutcome(
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
        judgeCache.put({ valid: verdict.valid, note: verdict.note }, ...cacheParts);
      }
      judged++;

      const note = `property-judge: ${verdict.note}`;
      if (!verdict.valid && ctx.result.verdict === "proven") {
        ctx.result.verdict = "violation";
        ctx.result.error = `encoding-inconsistent — ${verdict.note}`;
        flipped++;
        const lessons = new LessonStore(this.projectRoot);
        const principleMatch = ctx.claim.match(/^PROVEN: (\S+)/);
        lessons.add({
          contractKey: ctx.contractKey,
          claim: ctx.claim,
          judgeNote: verdict.note,
          principleId: principleMatch ? principleMatch[1]! : null,
        });
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

      const contract = store.get(ctx.contractKey);
      if (contract) {
        const propagate = (p: ProvenProperty | Violation) => {
          if (p.smt2 !== ctx.smt2) return;
          if (ctx.source === "proven" && verdict.valid) {
            p.confidence = "high";
            p.judge_note = `property-judge VALID: ${verdict.note}`;
          } else if (ctx.source === "proven" && !verdict.valid) {
            p.confidence = "low";
            p.judge_note = `property-judge INVALID (encoding-inconsistent): ${verdict.note}`;
          } else if (ctx.source === "violation" && !verdict.valid) {
            p.confidence = "high";
            p.judge_note = `property-judge confirmed bug: ${verdict.note}`;
          } else if (ctx.source === "violation" && verdict.valid) {
            p.confidence = "low";
            p.judge_note = `property-judge false positive: ${verdict.note}`;
          }
        };
        const target = ctx.source === "proven" ? contract.proven : contract.violations;
        target.forEach(propagate);
        touched.add(ctx.contractKey);
      }
    }

    for (const key of touched) {
      const c = store.get(key);
      if (c) store.put(c);
    }

    return { judged, flipped, confirmed, cacheHits };
  }

  async synthesizeAndRunHarnesses(model?: string): Promise<{
    attempted: number;
    pass: number;
    encodingGap: number;
    harnessError: number;
    untestable: number;
    synthesisFailed: number;
    timeout: number;
  }> {
    const stats = { attempted: 0, pass: 0, encodingGap: 0, harnessError: 0, untestable: 0, synthesisFailed: 0, timeout: 0 };
    if (process.env.NEURALLOG_HARNESS_SYNTHESIS !== "1") return stats;
    if (this.harnessCandidates.length === 0) return stats;

    const rawLimit = process.env.NEURALLOG_HARNESS_LIMIT;
    const limit = rawLimit ? Math.max(1, parseInt(rawLimit, 10) || 5) : 5;
    const timeoutMs = parseInt(process.env.NEURALLOG_HARNESS_TIMEOUT_MS || "3000", 10) || 3000;

    const provider = createProvider();
    const synthModel = model || process.env.NEURALLOG_HARNESS_MODEL || "claude-sonnet-4-6";
    const cache = new HarnessCache(this.projectRoot);
    const store = new ContractStore(this.projectRoot);
    const touched = new Set<string>();

    const candidates = this.harnessCandidates.slice(0, limit);
    const concurrency = Math.max(1, parseInt(process.env.NEURALLOG_HARNESS_CONCURRENCY || "3", 10) || 3);
    console.log(`[harness] ${candidates.length} candidate contracts (of ${this.harnessCandidates.length}); synthesizing with ${synthModel} at concurrency ${concurrency}`);

    type Prepared =
      | { cand: typeof candidates[number]; kind: "skip"; outcomeKind: HarnessOutcome["kind"]; message: string }
      | { cand: typeof candidates[number]; kind: "ready"; harness: string; info: FunctionInfo; absPath: string };

    const depsSourceCache = new Map<string, string>();
    const getDepsSource = (filePath: string): string => {
      if (depsSourceCache.has(filePath)) return depsSourceCache.get(filePath)!;
      const src = collectTransitiveSource(filePath, this.projectRoot, 1);
      depsSourceCache.set(filePath, src);
      return src;
    };

    const prepare = async (cand: typeof candidates[number]): Promise<Prepared> => {
      const absPath = this.resolvePath(cand.contract.file);
      const info = this.extractFunctionInfo(absPath, cand.contract.function);
      if (!info) return { cand, kind: "skip", outcomeKind: "synthesis-failed", message: "could not extract function source" };

      const depsSource = getDepsSource(absPath);
      let cached = cache.get(cand.prop.smt2, info.source, depsSource);
      let harness: string | null = cached?.harness || null;
      let untestable: string | null = cached?.untestable || null;

      if (!cached) {
        const testRefs = findTestsForFunction(this.projectRoot, cand.contract.function);
        const testOracleContext = testRefs.length > 0 ? formatTestOracleForPrompt(testRefs) : undefined;
        const result = await synthesizeHarness(
          {
            functionSource: info.source,
            claim: cand.prop.claim,
            smt2: cand.prop.smt2,
            contractKey: cand.contract.key,
            functionName: cand.contract.function,
            testOracleContext,
          },
          provider,
          synthModel,
          this.projectRoot
        );
        harness = result.harness;
        untestable = result.untestable;
        cache.put(cand.prop.smt2, info.source, { harness, untestable }, depsSource);
      }

      if (untestable) return { cand, kind: "skip", outcomeKind: "untestable", message: untestable };
      if (!harness) return { cand, kind: "skip", outcomeKind: "synthesis-failed", message: "LLM did not emit a valid harness or UNTESTABLE line" };

      if (process.env.NEURALLOG_HARNESS_AUDIT !== "0") {
        const auditModel = process.env.NEURALLOG_AUDIT_MODEL || process.env.NEURALLOG_JUDGE_MODEL || "claude-haiku-4-5-20251001";
        let auditValid = cached?.auditValid;
        let auditNote = cached?.auditNote || "";
        if (auditValid === undefined) {
          const verdict = await judgeHarnessCode(
            {
              harnessCode: harness,
              claim: cand.prop.claim,
              smt2: cand.prop.smt2,
              functionSource: info.source,
            },
            provider,
            auditModel
          );
          auditValid = verdict.valid;
          auditNote = verdict.note;
          cache.putAudit(cand.prop.smt2, info.source, verdict, depsSource);
        }
        if (!auditValid) {
          return { cand, kind: "skip", outcomeKind: "harness-error", message: `audit rejected: ${auditNote}` };
        }
      }

      return { cand, kind: "ready", harness, info, absPath };
    };

    const prepared = await this.parallelMap(candidates, concurrency, prepare);

    for (const p of prepared) {
      stats.attempted++;

      if (p.kind === "skip") {
        this.harnessResults.push(this.harnessCheckResult(p.cand, p.outcomeKind, p.message));
        if (p.outcomeKind === "untestable") stats.untestable++;
        else if (p.outcomeKind === "synthesis-failed") stats.synthesisFailed++;
        else if (p.outcomeKind === "harness-error") stats.harnessError++;
        this.applyHarnessVerdict(store, touched, p.cand, p.outcomeKind, p.message);
        continue;
      }

      const { cand, harness, info, absPath } = p;
      const extracted = this.loadFunction(absPath, cand.contract.function);
      if (!extracted) {
        this.harnessResults.push(this.harnessCheckResult(cand, "harness-error", "could not load function for harness execution"));
        stats.harnessError++;
        continue;
      }

      const fnClass = this.resolveClassFor(absPath, info.className);
      let outcome: HarnessOutcome;
      try {
        outcome = await runHarness(harness, extracted.fn, fnClass, timeoutMs);
      } catch (e: any) {
        outcome = { kind: "harness-error", message: String(e?.message || e).slice(0, 200), harnessCode: harness };
      }

      const result = this.harnessOutcomeToCheckResult(cand, outcome);

      if (process.env.NEURALLOG_RUN_ORACLE_TESTS === "1") {
        const framework = detectTestFramework(this.projectRoot);
        if (framework) {
          const refs = findTestsForFunction(this.projectRoot, cand.contract.function);
          if (refs.length > 0) {
            const oracleResults = await runTestsForReferences(
              this.projectRoot,
              framework,
              refs,
              absPath,
              { maxTests: 3, timeoutMsEach: 30000 }
            );
            const triangle = summarizeTriangle(outcome.kind, oracleResults);
            if (triangle.note) {
              result.error = result.error ? `${result.error}; ${triangle.note}` : triangle.note;
              if (triangle.hasDisagreement) {
                result.error = `triangle-disagreement: ${result.error}`;
              }
            }
          }
        }
      }

      this.harnessResults.push(result);

      if (outcome.kind === "pass") stats.pass++;
      else if (outcome.kind === "encoding-gap") stats.encodingGap++;
      else if (outcome.kind === "harness-error") stats.harnessError++;
      else if (outcome.kind === "timeout") stats.timeout++;

      this.applyHarnessVerdict(store, touched, cand, outcome.kind, outcome.message);
    }

    for (const key of touched) {
      const c = store.get(key);
      if (c) store.put(c);
    }

    return stats;
  }

  private harnessCheckResult(
    cand: { contract: Contract; prop: ProvenProperty | Violation; source: "proven" | "violation" },
    kind: HarnessOutcome["kind"],
    message: string
  ): CheckResult {
    const verdict: "proven" | "violation" | "error" =
      kind === "pass" ? "proven" :
      kind === "encoding-gap" ? "violation" :
      "error";
    return {
      checker: "property-test",
      description: `[${cand.prop.principle || "?"}] synth-harness ${cand.contract.function}:${cand.contract.line} — ${kind}: ${message.slice(0, 160)}`,
      sourceContract: `${cand.contract.function}:${cand.contract.line}`,
      smt2: cand.prop.smt2,
      expected: cand.source === "violation" ? "sat" : "unsat",
      z3Result: cand.source === "violation" ? "sat" : "unsat",
      verdict,
      error: kind === "pass" ? undefined : message,
    };
  }

  private harnessOutcomeToCheckResult(
    cand: { contract: Contract; prop: ProvenProperty | Violation; source: "proven" | "violation" },
    outcome: HarnessOutcome
  ): CheckResult {
    return this.harnessCheckResult(cand, outcome.kind, outcome.message);
  }

  private applyHarnessVerdict(
    store: ContractStore,
    touched: Set<string>,
    cand: { contract: Contract; prop: ProvenProperty | Violation; source: "proven" | "violation" },
    kind: HarnessOutcome["kind"],
    message: string
  ): void {
    const contract = store.get(cand.contract.key);
    if (!contract) return;
    const target = cand.source === "proven" ? contract.proven : contract.violations;
    for (const p of target) {
      if (p.smt2 !== cand.prop.smt2) continue;
      if (kind === "pass" && cand.source === "proven") {
        p.confidence = "high";
        p.judge_note = `harness-pass: ${message}`;
      } else if (kind === "encoding-gap") {
        p.confidence = "low";
        p.judge_note = `harness-encoding-gap: ${message.slice(0, 200)}`;
      } else if (kind === "untestable") {
        p.judge_note = `harness-untestable: ${message.slice(0, 200)}`;
      } else {
        p.judge_note = `harness-${kind}: ${message.slice(0, 200)}`;
      }
    }
    touched.add(cand.contract.key);
  }

  private resolveClassFor(filePath: string, className: string | null): any {
    if (!className) return null;
    try {
      const mod = require(filePath);
      return mod?.[className] || mod?.default?.[className] || null;
    } catch {
      return null;
    }
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

  private extractModels(smt2: string, k: number): Array<Record<string, number | boolean>> {
    const results: Array<Record<string, number | boolean>> = [];
    const first = this.extractModel(smt2);
    if (!first || Object.keys(first).length === 0) return results;
    results.push(first);

    if (k <= 1) return results;

    const lines = smt2.split("\n");
    const assertIndices: number[] = [];
    for (let i = 0; i < lines.length; i++) {
      if (lines[i]!.trim().startsWith("(assert")) assertIndices.push(i);
    }
    if (assertIndices.length === 0) return results;

    const goalIdx = assertIndices[assertIndices.length - 1]!;
    const baseLines = lines.filter((_, i) => i !== goalIdx).filter((l) => !l.includes("(check-sat)"));

    for (let iter = 1; iter < k; iter++) {
      const exclusions: string[] = [];
      for (const prev of results) {
        const disjuncts: string[] = [];
        for (const [name, val] of Object.entries(prev)) {
          if (typeof val === "number") {
            const lit = val < 0 ? `(- ${-val})` : String(val);
            disjuncts.push(`(not (= ${name} ${lit}))`);
          } else {
            disjuncts.push(`(not (= ${name} ${val}))`);
          }
        }
        if (disjuncts.length === 0) continue;
        exclusions.push(disjuncts.length === 1 ? `(assert ${disjuncts[0]})` : `(assert (or ${disjuncts.join(" ")}))`);
      }

      const query = [...baseLines, ...exclusions, "(check-sat)", "(get-model)"].join("\n");
      let output: string;
      try {
        output = execSync("z3 -in -T:5", { input: query, encoding: "utf-8", timeout: 6000 });
      } catch {
        return results;
      }
      if (!/\bsat\b/.test(output) || /\bunsat\b/.test(output)) return results;

      const nextModel = this.parseZ3Model(output);
      if (Object.keys(nextModel).length === 0) return results;

      const isDuplicate = results.some((prev) =>
        Object.keys(nextModel).every((k) => nextModel[k] === prev[k])
      );
      if (isDuplicate) return results;

      results.push(nextModel);
    }

    return results;
  }

  private parseZ3Model(text: string): Record<string, number | boolean> {
    const model: Record<string, number | boolean> = {};

    // Matches (define-fun NAME () SORT <value>) and captures the value payload
    // (which may be a plain literal or a parenthesized sexp like (- 5) or (/ 5 2)).
    const defRegex = /\(define-fun\s+(\S+)\s+\(\)\s+(Int|Real|Bool)\s+([\s\S]*?)\)\s*(?=\(define-fun|\)\s*$|\n\s*\))/g;
    let m;
    while ((m = defRegex.exec(text)) !== null) {
      const [, name, sort, rawValue] = m;
      const parsed = this.parseSexpValue(sort!, rawValue!.trim());
      if (parsed !== undefined) model[name!] = parsed;
    }

    // Simpler fallback regex catches values that didn't match the lookahead form
    // (last binding in the model, for instance).
    const simpleRegex = /\(define-fun\s+(\S+)\s+\(\)\s+(Int|Real|Bool)\s+([^)\n]+)\)/g;
    while ((m = simpleRegex.exec(text)) !== null) {
      const [, name, sort, rawValue] = m;
      if (model[name!] !== undefined) continue;
      const parsed = this.parseSexpValue(sort!, rawValue!.trim());
      if (parsed !== undefined) model[name!] = parsed;
    }

    return model;
  }

  private parseSexpValue(sort: string, raw: string): number | boolean | undefined {
    const trimmed = raw.trim();
    if (sort === "Bool") {
      if (trimmed === "true") return true;
      if (trimmed === "false") return false;
      return undefined;
    }

    if (!trimmed.startsWith("(")) {
      return this.parseValue(sort, trimmed);
    }

    // Strip outer parens, tokenize head
    const inner = trimmed.slice(1, -1).trim();
    const headMatch = inner.match(/^(\S+)\s+([\s\S]+)$/);
    if (!headMatch) return undefined;
    const [, op, rest] = headMatch;

    if (op === "-") {
      const sub = this.parseSexpValue(sort, rest!.trim());
      return sub === undefined ? undefined : (sub as number) * -1;
    }
    if (op === "/") {
      const parts = this.splitSexpArgs(rest!);
      if (parts.length !== 2) return undefined;
      const num = this.parseSexpValue(sort, parts[0]!);
      const den = this.parseSexpValue(sort, parts[1]!);
      if (typeof num !== "number" || typeof den !== "number" || den === 0) return undefined;
      return num / den;
    }
    return undefined;
  }

  private splitSexpArgs(s: string): string[] {
    const parts: string[] = [];
    let depth = 0;
    let current = "";
    for (const ch of s) {
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
      if (depth === 0 && /\s/.test(ch)) {
        if (current.length > 0) { parts.push(current); current = ""; }
        continue;
      }
      current += ch;
    }
    if (current.length > 0) parts.push(current);
    return parts;
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

    let fn = this.resolveCallable(mod, fnName, info, filePath);
    if (fn) {
      const result = { fn, paramNames: info.paramNames, paramTypes: info.paramTypes, source: info.source };
      this.fnCache.set(cacheKey, result);
      return result;
    }

    try {
      const modWithPrivates = loadModuleWithPrivates(filePath, require.main || undefined);
      fn = this.resolveCallable(modWithPrivates, fnName, info, filePath);
      if (fn) {
        const result = { fn, paramNames: info.paramNames, paramTypes: info.paramTypes, source: info.source };
        this.fnCache.set(cacheKey, result);
        return result;
      }
    } catch (e: any) {
      console.log(`[property-test] privates loader failed for ${filePath}: ${e?.message?.slice(0, 80) || "unknown"}`);
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
      const types: string[] = [];
      for (const child of paramsNode.namedChildren) {
        const patternNode = child.childForFieldName("pattern") || child.childForFieldName("name");
        const typeNode = child.childForFieldName("type");
        if (patternNode?.type === "identifier") {
          names.push(patternNode.text);
          types.push(typeNode ? typeNode.text.replace(/^:\s*/, "").trim() : "unknown");
        } else {
          return null;
        }
      }

      const isStatic = node.children.some((c) => c.text === "static" && c.type === "static");
      const className = this.findEnclosingClassName(node);

      return { paramNames: names, paramTypes: types, source: node.text, isStatic, className };
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
