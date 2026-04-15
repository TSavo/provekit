import { writeFileSync, mkdirSync, readFileSync } from "fs";
import { join, dirname, relative } from "path";
import { createHash } from "crypto";
import Handlebars from "handlebars";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { ContextBundle, CallSiteContext } from "./ContextPhase";
import { verifyAll, VerificationResult } from "../verifier";
import { PrincipleStore, hashPrinciple } from "../principles";
import { Contract, ContractStore, signalKey, ClauseHistory, ProvenProperty, Violation } from "../contracts";
import { computeSignalHash } from "../signals";
import { LLMProvider, createProvider } from "../llm";
import { DagExecutor } from "./DagExecutor";

export interface DerivationOutput {
  contracts: Contract[];
  newViolations: { violation: VerificationResult; context: string }[];
  derivedAt: string;
}

interface SignalNode {
  callSite: CallSiteContext;
  filePath: string;
  relativePath: string;
  key: string;
}

export interface DerivationInput {
  bundles: ContextBundle[];
  model: string;
  provider?: LLMProvider;
  maxConcurrency?: number;
}

export class DerivationPhase extends Phase<DerivationInput, DerivationOutput> {
  readonly name = "Contract Derivation";
  readonly phaseNumber = 3;

  private compiledTemplate: HandlebarsTemplateDelegate;

  constructor() {
    super();
    this.compiledTemplate = this.loadTemplate();
  }

  private loadTemplate(): HandlebarsTemplateDelegate {
    const candidates = [
      join(__dirname, "..", "..", "prompts", "invariant_derivation.md"),
      join(process.cwd(), "prompts", "invariant_derivation.md"),
    ];

    for (const path of candidates) {
      try {
        const raw = readFileSync(path, "utf-8");
        const promptStart = raw.indexOf("## Prompt\n");
        const template = promptStart !== -1
          ? raw.slice(promptStart + "## Prompt\n\n".length)
          : raw;
        return Handlebars.compile(template, { noEscape: true });
      } catch { continue; }
    }

    throw new Error("Could not find prompts/invariant_derivation.md");
  }

  async execute(input: DerivationInput, options: PhaseOptions): Promise<PhaseResult<DerivationOutput>> {
    const { bundles, model } = input;
    const provider = input.provider || createProvider();
    const maxConcurrency = input.maxConcurrency || 5;

    this.log("Deriving contracts...");
    this.detail(`Provider: ${provider.name}`);

    const principleStore = new PrincipleStore(options.projectRoot);
    const discoveredPrinciples = principleStore.formatForPrompt();
    const principleHash = principleStore.computePrincipleHash();

    this.detail(`Model: ${model}`);
    this.detail(`Principles: 7 seed + ${principleStore.getAll().length} discovered`);

    const store = new ContractStore(options.projectRoot);

    const signals: SignalNode[] = [];
    for (const bundle of bundles) {
      for (const callSite of bundle.callSites) {
        const key = signalKey(bundle.relativePath, callSite.functionName, callSite.line);
        signals.push({ callSite, filePath: bundle.filePath, relativePath: bundle.relativePath, key });
      }
    }

    this.detail(`Signals: ${signals.length} total`);

    const signalsByFunction = new Map<string, SignalNode[]>();
    for (const s of signals) {
      const fn = s.callSite.functionName;
      if (!signalsByFunction.has(fn)) signalsByFunction.set(fn, []);
      signalsByFunction.get(fn)!.push(s);
    }

    const dag = new DagExecutor<SignalNode, Contract>(maxConcurrency);
    let depEdges = 0;

    for (const signal of signals) {
      const callees = signal.callSite.callees || [];
      const deps: string[] = [];
      for (const calleeName of callees) {
        const targets = signalsByFunction.get(calleeName);
        if (targets) {
          for (const target of targets) {
            if (target.key !== signal.key) {
              deps.push(target.key);
            }
          }
        }
      }
      depEdges += deps.length;
      dag.add({ key: signal.key, data: signal, dependsOn: deps });
    }

    console.log(`  DAG: ${signals.length} signals, ${depEdges} call-graph edges, max ${maxConcurrency} concurrent`);
    console.log(`  Each signal waits for signals in functions it calls to resolve`);
    console.log();

    const allContracts: Contract[] = [];
    const allNewViolations: { violation: VerificationResult; context: string }[] = [];
    let completed = 0;
    const startTime = Date.now();
    const systemPrompt = `You are a formal verification engine. Produce SMT-LIB 2 formulas. Every block MUST use \`\`\`smt2 fences and include (check-sat). Tag every block with ; PRINCIPLE: P1-P7 or [NEW].`;

    await dag.execute(async (node, resolvedDeps) => {
      const { callSite, filePath, relativePath, key } = node.data;
      completed++;

      const depKeys = [...resolvedDeps.keys()];
      const depContracts = [...resolvedDeps.values()];
      const contextAccumulated = store.formatForPrompt(depKeys);

      this.printProgress(completed, signals.length, callSite, startTime, maxConcurrency);

      const deriveStart = Date.now();
      const prompt = this.buildPrompt(callSite, filePath, contextAccumulated, discoveredPrinciples);
      const response = await provider.complete(prompt, { model, systemPrompt });
      const deriveMs = Date.now() - deriveStart;

      const verifyStart = Date.now();
      const verifications = verifyAll(response.text);
      const verifyMs = Date.now() - verifyStart;

      const contract = this.buildContract(key, filePath, callSite, verifications, depKeys);
      console.log(`      derived ${this.formatDuration(deriveMs)} -> verified ${this.formatDuration(verifyMs)} -> resolved (${contract.proven.length + contract.violations.length} proofs)`);

      const newViolations = verifications
        .filter((v) => v.z3Result === "sat" && v.principle?.toUpperCase().includes("NEW"))
        .map((v) => ({ violation: v, context: key }));

      this.printResult(verifications, newViolations.length);

      return contract;
    }, (key, contract) => {
      allContracts.push(contract);
      store.put(contract);

      const newViolations = contract.violations
        .filter((v) => v.principle?.toUpperCase().includes("NEW"))
        .map((v) => ({ violation: { smt2: v.smt2, z3Result: "sat" as const, principle: v.principle, error: undefined }, context: key }));
      allNewViolations.push(...newViolations);

      const p = contract.proven.length;
      const v = contract.violations.length;
      if (p + v > 0) {
        console.log(`  >> ${key} resolved: ${p} proven, ${v} violations`);
      }
    });

    const output: DerivationOutput = {
      contracts: allContracts,
      newViolations: allNewViolations,
      derivedAt: new Date().toISOString(),
    };

    const outPath = join(options.projectRoot, ".neurallog", "derivation.json");
    writeFileSync(outPath, JSON.stringify({ derivedAt: output.derivedAt, contractCount: allContracts.length }, null, 2));

    const totalProven = allContracts.reduce((n, c) => n + c.proven.length, 0);
    const totalViolations = allContracts.reduce((n, c) => n + c.violations.length, 0);
    this.detail(`Derivation complete:`);
    this.detail(`  ${allContracts.length} contracts`);
    this.detail(`  ${totalProven} proven (unsat) | ${totalViolations} violations (sat)`);
    this.detail(`  ${allNewViolations.length} [NEW] violations for Phase 4`);
    console.log();

    return { data: output, writtenTo: outPath };
  }

  private buildPrompt(
    callSite: CallSiteContext,
    filePath: string,
    accumulated: string,
    discoveredPrinciples: string
  ): string {
    const importSources = callSite.importSources.length > 0
      ? callSite.importSources
          .map((imp) => `#### ${imp.path}\n\`\`\`typescript\n${imp.source}\n\`\`\``)
          .join("\n\n")
      : "(no imports)";

    let enrichedContext = callSite.callingContext;

    if (callSite.typeContext && callSite.typeContext !== "(no type annotations found)") {
      enrichedContext += `\n\nType information (from TypeScript AST):\n${callSite.typeContext}`;
    }

    if (callSite.pathConditions && callSite.pathConditions.length > 0) {
      enrichedContext += `\n\nPath conditions (must be true for execution to reach this signal):\n`;
      enrichedContext += callSite.pathConditions.map((c, i) => `  ${i + 1}. ${c}`).join("\n");
      enrichedContext += `\nThese are KNOWN FACTS at this signal -- the code guarantees them.`;
    }

    let prompt = this.compiledTemplate({
      TARGET_FILE: filePath,
      TARGET_FUNCTION: callSite.functionName,
      TARGET_LINE: String(callSite.line),
      TARGET_STATEMENT: callSite.signalText,
      TARGET_FILE_SOURCE: callSite.fileSource,
      IMPORT_SOURCES: importSources,
      EXISTING_CONTRACTS: accumulated,
      CALLING_CONTEXT: enrichedContext,
    });

    if (discoveredPrinciples) {
      const insertPoint = prompt.indexOf("### SMT-LIB 2 Grammar");
      if (insertPoint !== -1) {
        prompt = prompt.slice(0, insertPoint) + discoveredPrinciples + "\n\n" + prompt.slice(insertPoint);
      }
    }

    return prompt;
  }

  private buildContract(
    key: string,
    file: string,
    callSite: CallSiteContext,
    verifications: VerificationResult[],
    dependencyKeys: string[]
  ): Contract {
    const proven: ProvenProperty[] = [];
    const violations: Violation[] = [];

    for (const v of verifications) {
      const commentLines = v.smt2.split("\n")
        .filter((l) => l.trim().startsWith(";"))
        .map((l) => l.trim().replace(/^;\s*/, ""));

      const claim = commentLines.find((l) => !l.startsWith("PRINCIPLE:") && l.length > 10) || "(no claim extracted)";
      const pHash = v.principle ? this.resolvePrincipleHash(v.principle) : "";

      if (v.z3Result === "unsat") {
        proven.push({ principle: v.principle, principle_hash: pHash, claim, smt2: v.smt2 });
      } else if (v.z3Result === "sat") {
        violations.push({ principle: v.principle, principle_hash: pHash, claim, smt2: v.smt2 });
      }
    }

    return {
      key,
      file,
      function: callSite.functionName,
      line: callSite.line,
      signal_hash: callSite.signalHash,
      proven,
      violations,
      depends_on: dependencyKeys,
      clause_history: [
        ...proven.map((p) => ({ clause: p.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
        ...violations.map((v) => ({ clause: v.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
      ],
    };
  }

  private resolvePrincipleHash(principleTag: string): string {
    const ids = principleTag.replace(/\[NEW\]/gi, "").split(/[,+&\s]+/).map((s) => s.trim()).filter((s) => /^P\d+$/i.test(s));
    if (ids.length === 0) return "";
    if (ids.length === 1) return hashPrinciple(ids[0]!);
    const combined = createHash("sha256");
    for (const id of ids.sort()) { combined.update(id); combined.update(hashPrinciple(id)); }
    return combined.digest("hex");
  }

  private etaBaseTime: number = 0;
  private etaBaseCompleted: number = 0;

  private printProgress(completed: number, total: number, callSite: CallSiteContext, startTime: number, concurrency: number = 1): void {
    const pct = Math.round((completed / total) * 100);
    const filled = Math.round((completed / total) * 20);
    const bar = "\u2588".repeat(filled) + "\u2591".repeat(20 - filled);
    if (this.etaBaseTime === 0 && completed > concurrency) {
      this.etaBaseTime = Date.now();
      this.etaBaseCompleted = completed;
    }
    let etaStr = "...";
    if (this.etaBaseTime > 0 && completed > this.etaBaseCompleted) {
      const since = Date.now() - this.etaBaseTime;
      const done = completed - this.etaBaseCompleted;
      etaStr = this.formatDuration(((total - completed) * since) / done);
    }
    process.stdout.write(`\r    [${bar}] ${completed}/${total} (${pct}%) ${callSite.functionName}:${callSite.line} ETA ${etaStr}    \n`);
  }

  private printResult(verifications: VerificationResult[], newCount: number): void {
    const p = verifications.filter((v) => v.z3Result === "unsat").length;
    const v = verifications.filter((v) => v.z3Result === "sat").length;
    process.stdout.write(`      -> ${verifications.length} blocks: ${p} proven ${v} violations${newCount > 0 ? ` (${newCount} [NEW])` : ""}\n`);
  }

  private formatDuration(ms: number): string {
    if (ms < 1000) return `${Math.round(ms)}ms`;
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ${s % 60}s`;
    return `${Math.floor(m / 60)}h ${m % 60}m`;
  }
}

