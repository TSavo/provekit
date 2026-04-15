import { writeFileSync, mkdirSync, readFileSync } from "fs";
import { join, dirname, relative } from "path";
import { createHash } from "crypto";
import Handlebars from "handlebars";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { ContextBundle, CallSiteContext } from "./ContextPhase";
import { verifyAll, VerificationResult } from "../verifier";
import { PrincipleStore } from "../principles";
import { Contract, ContractStore, signalKey, ClauseHistory, ProvenProperty, Violation } from "../contracts";
import { computeSignalHash } from "../signals";
import { LLMProvider, createProvider } from "../llm";
import { classifyAndGeneralize } from "../principles";
import { ObservationStore } from "../observations";
import { DagExecutor } from "./DagExecutor";
import { buildSignalFrame } from "./PromptStrategy";
import { assembleDossier, formatDossier } from "./Dossier";

export interface DerivationOutput {
  contracts: Contract[];
  newViolations: { violation: VerificationResult; context: string }[];
  derivedAt: string;
}

interface FunctionNode {
  functionName: string;
  filePath: string;
  relativePath: string;
  signals: { callSite: CallSiteContext; key: string }[];
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
    let discoveredPrinciples = principleStore.formatForPrompt();
    const observationStore = new ObservationStore(options.projectRoot);

    this.detail(`Model: ${model}`);
    this.detail(`Principles: ${principleStore.getPrincipleCount()} (${principleStore.getAll().length} discovered), ${observationStore.getAll().length} observations`);

    const store = new ContractStore(options.projectRoot);

    const functionNodes = new Map<string, FunctionNode>();
    let totalSignals = 0;

    for (const bundle of bundles) {
      for (const callSite of bundle.callSites) {
        const fnKey = `${bundle.relativePath}/${callSite.functionName}`;
        if (!functionNodes.has(fnKey)) {
          functionNodes.set(fnKey, {
            functionName: callSite.functionName,
            filePath: bundle.filePath,
            relativePath: bundle.relativePath,
            signals: [],
          });
        }
        const key = signalKey(bundle.relativePath, callSite.functionName, callSite.line);
        functionNodes.get(fnKey)!.signals.push({ callSite, key });
        totalSignals++;
      }
    }

    this.detail(`${totalSignals} signals in ${functionNodes.size} functions`);

    const dag = new DagExecutor<FunctionNode, Contract[]>(maxConcurrency);
    let depEdges = 0;

    for (const [fnKey, node] of functionNodes) {
      const calleesSet = new Set<string>();
      for (const s of node.signals) {
        for (const c of s.callSite.callees || []) calleesSet.add(c);
      }
      const deps: string[] = [];
      for (const calleeName of calleesSet) {
        for (const [otherKey, otherNode] of functionNodes) {
          if (otherKey !== fnKey && otherNode.functionName === calleeName) {
            deps.push(otherKey);
          }
        }
      }
      depEdges += deps.length;
      dag.add({ key: fnKey, data: node, dependsOn: deps });
    }

    console.log(`  DAG: ${functionNodes.size} functions, ${totalSignals} signals, ${depEdges} call-graph edges, max ${maxConcurrency} concurrent`);
    console.log(`  One LLM call per function. Each function waits for functions it calls to resolve.`);
    console.log();

    const allContracts: Contract[] = [];
    const allNewViolations: { violation: VerificationResult; context: string }[] = [];
    let completedFunctions = 0;
    const startTime = Date.now();
    const principleCount = principleStore.getPrincipleCount();
    const systemPrompt = `You are a formal verification engine. Produce SMT-LIB 2 formulas.

Every block MUST:
- Use \`\`\`smt2 fences
- Include (check-sat)
- Tag with ; PRINCIPLE: <id> or [NEW]
- Tag with ; LINE: <number>

There are ${principleCount} known principles (P1-P${principleCount}). If a violation genuinely does not fit ANY existing principle — do NOT stretch a principle to fit. Tag it [NEW]. Novel patterns are valuable. Examples of [NEW]: resource lifecycle (open without close), state machine violations (invalid transitions), ordering constraints, information flow, idempotency failures. If you have to argue why a principle applies, it's [NEW].`;

    await dag.execute(async (node, resolvedDeps) => {
      const fn = node.data;
      completedFunctions++;

      const depContracts: Contract[] = [];
      for (const [, contracts] of resolvedDeps) {
        depContracts.push(...contracts);
      }
      const depKeys = depContracts.map((c) => c.key);
      const contextAccumulated = store.formatForPrompt(depKeys);

      const pct = Math.round((completedFunctions / functionNodes.size) * 100);
      console.log(`  [${completedFunctions}/${functionNodes.size}] (${pct}%) ${fn.relativePath}/${fn.functionName} — ${fn.signals.length} signals, ${resolvedDeps.size} deps`);

      const callSites = fn.signals.map((s) => s.callSite);
      const signalFrame = buildSignalFrame(callSites);
      const dossier = assembleDossier(callSites, fn.filePath, options.projectRoot, store);
      const dossierText = formatDossier(dossier);

      const observationsContext = observationStore.formatForPrompt();

      const deriveStart = Date.now();
      const prompt = this.buildPrompt(callSites[0]!, fn.filePath, contextAccumulated, discoveredPrinciples + observationsContext, signalFrame, dossierText);
      const response = await provider.complete(prompt, { model, systemPrompt });
      const deriveMs = Date.now() - deriveStart;

      const verifyStart = Date.now();
      const verifications = verifyAll(response.text);
      const verifyMs = Date.now() - verifyStart;

      const contracts = this.buildContracts(fn, verifications, depKeys, principleStore);

      for (const contract of contracts) {
        store.put(contract);
      }

      // Inline Phase 4: classify [NEW] violations immediately
      for (const contract of contracts) {
        for (const v of contract.violations) {
          if (!v.principle?.toUpperCase().includes("NEW")) continue;

          const obsId = observationStore.nextId();
          observationStore.add({
            id: obsId,
            signalKey: contract.key,
            claim: v.claim,
            smt2: v.smt2,
            rejectedPrincipleName: "",
            rejectedPrincipleDescription: "",
            adversaryFeedback: "",
            observedAt: new Date().toISOString(),
          });
          console.log(`    [NEW] observation ${obsId} in ${contract.key} — attempting to generalize into principle...`);

          const violation = { smt2: v.smt2, z3Result: "sat" as const, principle: v.principle, error: undefined };
          const principle = await classifyAndGeneralize(
            violation, contract.key, principleStore.getAll(), model, provider
          );

          if (principle) {
            principle.id = principleStore.nextId();
            if (principle.validated) {
              principleStore.add(principle);
              discoveredPrinciples = principleStore.formatForPrompt();
              console.log(`    PROMOTED: observation ${obsId} → ${principle.id} — ${principle.name}`);
              console.log(`    Subsequent derivations will use this principle.`);
            } else {
              console.log(`    REJECTED as principle: ${principle.name}`);
              console.log(`    Observation ${obsId} remains (the bug is real, the generalization didn't survive)`);
            }
          }
        }
      }

      const totalProofs = contracts.reduce((n, c) => n + c.proven.length + c.violations.length, 0);
      console.log(`    derived ${this.formatDuration(deriveMs)} -> verified ${this.formatDuration(verifyMs)} -> ${contracts.length} contracts, ${totalProofs} proofs`);

      return contracts;
    }, (fnKey, contracts) => {
      allContracts.push(...contracts);
      for (const c of contracts) {
        const newViolations = c.violations
          .filter((v) => v.principle?.toUpperCase().includes("NEW"))
          .map((v) => ({ violation: { smt2: v.smt2, z3Result: "sat" as const, principle: v.principle, error: undefined }, context: c.key }));
        allNewViolations.push(...newViolations);
      }
      const p = contracts.reduce((n, c) => n + c.proven.length, 0);
      const v = contracts.reduce((n, c) => n + c.violations.length, 0);
      if (p + v > 0) {
        console.log(`    >> ${fnKey}: ${p} proven, ${v} violations`);
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
    const unattributed = allContracts.filter((c) => c.proven.length === 0 && c.violations.length === 0).length;
    this.detail(`Derivation complete: ${this.formatDuration(Date.now() - startTime)}`);
    this.detail(`  ${allContracts.length} contracts across ${functionNodes.size} functions`);
    this.detail(`  ${totalProven} proven (unsat) | ${totalViolations} violations (sat) | ${unattributed} unattributed`);
    this.detail(`  ${allNewViolations.length} [NEW] violations for Phase 4`);
    if (unattributed > 0) {
      this.detail(`  WARNING: ${unattributed} signals received zero attributed SMT-LIB blocks`);
    }
    console.log();

    return { data: output, writtenTo: outPath };
  }

  private buildPrompt(
    representative: CallSiteContext,
    filePath: string,
    accumulated: string,
    discoveredPrinciples: string,
    signalFrame: string,
    dossierText: string
  ): string {
    const importSources = representative.importSources.length > 0
      ? representative.importSources.map((imp) => `#### ${imp.path}\n\`\`\`typescript\n${imp.source}\n\`\`\``).join("\n\n")
      : "(no imports)";

    let enrichedContext = representative.callingContext;
    if (representative.typeContext && representative.typeContext !== "(no type annotations found)") {
      enrichedContext += `\n\nType information (from TypeScript AST):\n${representative.typeContext}`;
    }
    if (representative.pathConditions && representative.pathConditions.length > 0) {
      enrichedContext += `\n\nPath conditions:\n` + representative.pathConditions.map((c, i) => `  ${i + 1}. ${c}`).join("\n");
    }

    enrichedContext += `\n\n${signalFrame}`;

    if (dossierText) {
      enrichedContext += `\n\n${dossierText}`;
    }

    let prompt = this.compiledTemplate({
      TARGET_FILE: filePath,
      TARGET_FUNCTION: representative.functionName,
      TARGET_LINE: String(representative.line),
      TARGET_STATEMENT: signalFrame,
      TARGET_FILE_SOURCE: representative.fileSource,
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

  private buildContracts(
    fn: FunctionNode,
    verifications: VerificationResult[],
    dependencyKeys: string[],
    principleStore: PrincipleStore
  ): Contract[] {
    const contracts: Contract[] = [];

    for (const { callSite, key } of fn.signals) {
      const lineVerifications = verifications.filter((v) => {
        const lineMatch = v.smt2.match(/;\s*LINE:\s*(\d+)/i);
        if (lineMatch) return parseInt(lineMatch[1]!, 10) === callSite.line;
        return false;
      });

      const unmatched = lineVerifications.length === 0;

      const toUse = unmatched && fn.signals.length === 1 ? verifications : lineVerifications;

      const proven: ProvenProperty[] = [];
      const violations: Violation[] = [];

      for (const v of toUse) {
        const commentLines = v.smt2.split("\n").filter((l) => l.trim().startsWith(";")).map((l) => l.trim().replace(/^;\s*/, ""));
        const claim = commentLines.find((l) => !l.startsWith("PRINCIPLE:") && !l.startsWith("LINE:") && l.length > 10) || "(no claim extracted)";
        const pHash = v.principle ? this.resolvePrincipleHash(v.principle, principleStore) : "";

        if (v.z3Result === "unsat") {
          proven.push({ principle: v.principle, principle_hash: pHash, claim, smt2: v.smt2 });
        } else if (v.z3Result === "sat") {
          violations.push({ principle: v.principle, principle_hash: pHash, claim, smt2: v.smt2 });
        }
      }

      contracts.push({
        key,
        file: fn.filePath,
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
      });
    }

    return contracts;
  }

  private resolvePrincipleHash(principleTag: string, store: PrincipleStore): string {
    const ids = principleTag.replace(/\[NEW\]/gi, "").split(/[,+&\s]+/).map((s) => s.trim()).filter((s) => /^P\d+$/i.test(s));
    if (ids.length === 0) return "";
    if (ids.length === 1) return store.hashForPrinciple(ids[0]!);
    const combined = createHash("sha256");
    for (const id of ids.sort()) { combined.update(id); combined.update(store.hashForPrinciple(id)); }
    return combined.digest("hex");
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
