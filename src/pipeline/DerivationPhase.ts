import { writeFileSync, mkdirSync, readFileSync } from "fs";
import { join, dirname, relative } from "path";
import { createHash } from "crypto";
import Handlebars from "handlebars";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { ContextBundle, CallSiteContext } from "./ContextPhase";
import { verifyAll, VerificationResult } from "../verifier";
import { PrincipleStore, hashPrinciple } from "../principles";
import { ClauseHistory } from "../contracts";
import { computeSignalHash } from "../signals";
import { LLMProvider, createProvider } from "../llm";
import { DagExecutor } from "./DagExecutor";

export interface DerivedContract {
  file: string;
  function: string;
  line: number;
  signal_hash: string;
  proven: { principle: string | null; principle_hash: string; claim: string; smt2: string }[];
  violations: { principle: string | null; principle_hash: string; claim: string; smt2: string }[];
  depends_on: string[];
  clause_history: ClauseHistory[];
}

export interface DerivationOutput {
  contracts: DerivedContract[];
  newViolations: { violation: VerificationResult; context: string }[];
  derivedAt: string;
}

interface BundleResult {
  contracts: DerivedContract[];
  newViolations: { violation: VerificationResult; context: string }[];
}

export interface DerivationInput {
  bundles: ContextBundle[];
  model: string;
  provider?: LLMProvider;
  parallelGroups?: { depth: number; files: string[] }[];
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

    this.log("Deriving contracts...");
    this.detail(`Provider: ${provider.name}`);

    const principleStore = new PrincipleStore(options.projectRoot);
    const discoveredPrinciples = principleStore.formatForPrompt();
    const principleHash = principleStore.computePrincipleHash();
    const discoveredCount = principleStore.getAll().length;

    this.detail(`Model: ${model}`);
    this.detail(`Principles: 7 seed${discoveredCount > 0 ? ` + ${discoveredCount} discovered` : ""}`);
    this.detail(`Principle hash: ${principleHash}`);
    const totalCallSites = bundles.reduce((n, b) => n + b.callSites.length, 0);
    this.detail(`Bundles: ${bundles.length} files, ${totalCallSites} call sites`);
    console.log();

    const allContracts: DerivedContract[] = [];
    const allNewViolations: { violation: VerificationResult; context: string }[] = [];
    let completed = 0;
    const startTime = Date.now();
    const systemPrompt = `You are a formal verification engine. Produce SMT-LIB 2 formulas. Every block MUST use \`\`\`smt2 fences and include (check-sat). Tag every block with ; PRINCIPLE: P1-P7 or [NEW].`;

    const groups = input.parallelGroups;
    const maxConcurrency = input.maxConcurrency || 5;

    if (groups && groups.length > 0 && bundles.length > 1) {
      console.log(`  DAG executor: ${bundles.length} files, max ${maxConcurrency} concurrent`);
      console.log(`  Each file starts only after all its imports are fully resolved (derived + Z3 verified)`);
      console.log(`  The moment a file resolves, any file waiting only on it starts immediately`);
      console.log();

      const dag = new DagExecutor<ContextBundle, BundleResult>(maxConcurrency);

      const bundleByFile = new Map(bundles.map((b) => [b.filePath, b]));

      for (const bundle of bundles) {
        const importPaths = bundle.callSites[0]?.importSources.map((imp) => imp.path) || [];
        const deps: string[] = [];
        for (const [filePath] of bundleByFile) {
          if (filePath === bundle.filePath) continue;
          const relPath = relative(options.projectRoot, filePath);
          if (importPaths.some((imp) => imp === relPath || relPath.endsWith(imp) || imp.endsWith(relPath))) {
            deps.push(filePath);
          }
        }
        dag.add({ key: bundle.filePath, data: bundle, dependsOn: deps });
        if (deps.length > 0) {
          console.log(`  ${bundle.relativePath} waits for: ${deps.map((d) => relative(options.projectRoot, d)).join(", ")}`);
        } else {
          console.log(`  ${bundle.relativePath} -- no import dependencies, eligible immediately`);
        }
      }
      console.log();

      await dag.execute(async (node, resolvedDeps) => {
        const bundle = node.data;

        const depContracts: DerivedContract[] = [];
        for (const [, depResult] of resolvedDeps) {
          depContracts.push(...depResult.contracts);
        }
        const contextContracts = [...allContracts, ...depContracts];
        const contextAccumulated = this.formatAccumulated(contextContracts);

        console.log(`  ${bundle.relativePath}: ${bundle.callSites.length} signals (${resolvedDeps.size} resolved deps in context)`);

        const bundleContracts: DerivedContract[] = [];
        const bundleNewViolations: { violation: VerificationResult; context: string }[] = [];

        for (const callSite of bundle.callSites) {
          completed++;
          this.printProgress(completed, totalCallSites, callSite, startTime, maxConcurrency);

          const deriveStart = Date.now();
          const prompt = this.buildPrompt(callSite, bundle.filePath, contextAccumulated, discoveredPrinciples);
          const response = await provider.complete(prompt, { model, systemPrompt });
          const deriveMs = Date.now() - deriveStart;

          const verifyStart = Date.now();
          const verifications = verifyAll(response.text);
          const verifyMs = Date.now() - verifyStart;

          const contract = this.buildContract(bundle.filePath, callSite.functionName, callSite.line, callSite.signalHash, verifications, [...contextContracts, ...bundleContracts]);
          bundleContracts.push(contract);

          console.log(`      derived ${this.formatDuration(deriveMs)} -> verified ${this.formatDuration(verifyMs)} -> resolved (${contract.proven.length + contract.violations.length} proofs)`);

          const newViolations = verifications
            .filter((v) => v.z3Result === "sat" && v.principle?.toUpperCase().includes("NEW"))
            .map((v) => ({ violation: v, context: `${bundle.filePath}:${callSite.functionName}:${callSite.line}` }));
          bundleNewViolations.push(...newViolations);

          this.printResult(verifications, newViolations.length);
        }

        this.writeContractsForFile(bundle.filePath, [...contextContracts, ...bundleContracts], options.projectRoot, principleHash);
        return { contracts: bundleContracts, newViolations: bundleNewViolations };
      }, (key, result) => {
        allContracts.push(...result.contracts);
        allNewViolations.push(...result.newViolations);
        const proven = result.contracts.reduce((n, c) => n + c.proven.length, 0);
        const violations = result.contracts.reduce((n, c) => n + c.violations.length, 0);
        console.log(`  >> ${relative(options.projectRoot, key)} resolved: ${result.contracts.length} contracts (${proven} proven, ${violations} violations) -- dependents unblocked`);
      });
    } else {
      for (const bundle of bundles) {
        this.detail(`${bundle.relativePath}:`);
        let accumulated = this.formatAccumulated(allContracts);

        for (const callSite of bundle.callSites) {
          completed++;
          this.printProgress(completed, totalCallSites, callSite, startTime);

          const prompt = this.buildPrompt(callSite, bundle.filePath, accumulated, discoveredPrinciples);

          let rawResponse = "";
          if (options.verbose) {
            for await (const event of provider.stream(prompt, { model, systemPrompt })) {
              if (event.type === "text_delta" && event.text) {
                process.stdout.write(event.text);
              }
              if (event.type === "done" && event.text) {
                rawResponse = event.text;
              }
            }
          } else {
            const response = await provider.complete(prompt, { model, systemPrompt });
            rawResponse = response.text;
          }

          const verifications = verifyAll(rawResponse);
          const contract = this.buildContract(bundle.filePath, callSite.functionName, callSite.line, callSite.signalHash, verifications, allContracts);
          allContracts.push(contract);

          const newViolations = verifications
            .filter((v) => v.z3Result === "sat" && v.principle?.toUpperCase().includes("NEW"))
            .map((v) => ({ violation: v, context: `${bundle.filePath}:${callSite.functionName}:${callSite.line}` }));
          allNewViolations.push(...newViolations);

          accumulated = this.formatAccumulated(allContracts);
          this.printResult(verifications, newViolations.length);
        }

        this.writeContractsForFile(bundle.filePath, allContracts, options.projectRoot, principleHash);
      }
    }

    const output: DerivationOutput = {
      contracts: allContracts,
      newViolations: allNewViolations,
      derivedAt: new Date().toISOString(),
    };

    const outPath = join(options.projectRoot, ".neurallog", "derivation.json");
    writeFileSync(outPath, JSON.stringify(output, null, 2));

    const totalProven = allContracts.reduce((n, c) => n + c.proven.length, 0);
    const totalViolations = allContracts.reduce((n, c) => n + c.violations.length, 0);
    this.detail(`Derivation complete:`);
    this.detail(`  ${allContracts.length} contracts across ${bundles.length} files`);
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
    file: string,
    functionName: string,
    line: number,
    signalHash: string,
    verifications: VerificationResult[],
    priorContracts: DerivedContract[]
  ): DerivedContract {
    const proven: DerivedContract["proven"] = [];
    const violations: DerivedContract["violations"] = [];

    for (const v of verifications) {
      const commentLines = v.smt2
        .split("\n")
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

    const depends_on = priorContracts.map((c) => {
      const content = c.proven.map((p) => p.smt2).join("\n") + c.violations.map((v) => v.smt2).join("\n");
      return createHash("sha256").update(content).digest("hex");
    });

    return {
      file,
      function: functionName,
      line,
      signal_hash: signalHash,
      proven,
      violations,
      depends_on,
      clause_history: [
        ...proven.map((p) => ({ clause: p.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
        ...violations.map((v) => ({ clause: v.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
      ],
    };
  }

  private resolvePrincipleHash(principleTag: string): string {
    const ids = principleTag
      .replace(/\[NEW\]/gi, "")
      .split(/[,+&\s]+/)
      .map((s) => s.trim())
      .filter((s) => /^P\d+$/i.test(s));

    if (ids.length === 0) return "";
    if (ids.length === 1) return hashPrinciple(ids[0]!);

    const combined = createHash("sha256");
    for (const id of ids.sort()) {
      combined.update(id);
      combined.update(hashPrinciple(id));
    }
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
      const sinceSteadyState = Date.now() - this.etaBaseTime;
      const completedSinceSteadyState = completed - this.etaBaseCompleted;
      const etaMs = ((total - completed) * sinceSteadyState) / completedSinceSteadyState;
      etaStr = this.formatDuration(etaMs);
    }

    process.stdout.write(
      `\r    [${bar}] ${completed}/${total} (${pct}%) ${callSite.functionName}:${callSite.line} ETA ${etaStr}    \n`
    );
  }

  private printResult(verifications: VerificationResult[], newCount: number): void {
    const provenCount = verifications.filter((v) => v.z3Result === "unsat").length;
    const violationCount = verifications.filter((v) => v.z3Result === "sat").length;
    process.stdout.write(
      `      -> ${verifications.length} blocks: ${provenCount} proven ${violationCount} violations` +
        (newCount > 0 ? ` (${newCount} [NEW])` : "") + "\n"
    );
  }

  private formatDuration(ms: number): string {
    if (ms < 1000) return `${Math.round(ms)}ms`;
    const seconds = Math.floor(ms / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    if (minutes < 60) return `${minutes}m ${remainingSeconds}s`;
    const hours = Math.floor(minutes / 60);
    const remainingMinutes = minutes % 60;
    return `${hours}h ${remainingMinutes}m`;
  }

  private formatAccumulated(contracts: DerivedContract[]): string {
    if (contracts.length === 0) return "(no existing contracts yet -- first pass)";

    return contracts.map((c) => {
      const lines: string[] = [];
      lines.push(`### ${c.file}:${c.function} (line ${c.line})`);
      if (c.proven.length > 0) {
        lines.push("\nProven (Z3 unsat):");
        for (const p of c.proven) lines.push(`  [${p.principle || "?"}] ${p.claim}`);
      }
      if (c.violations.length > 0) {
        lines.push("\nViolations (Z3 sat):");
        for (const v of c.violations) lines.push(`  [${v.principle || "?"}] ${v.claim}`);
      }
      return lines.join("\n");
    }).join("\n\n");
  }

  private writeContractsForFile(
    filePath: string,
    allContracts: DerivedContract[],
    projectRoot: string,
    principleHash?: string
  ): void {
    const relPath = relative(projectRoot, filePath);
    const contractPath = join(projectRoot, ".neurallog", "contracts", relPath + ".json");
    const dir = dirname(contractPath);
    mkdirSync(dir, { recursive: true });

    const fileSource = readFileSync(filePath, "utf-8");
    const fileHash = createHash("sha256").update(fileSource).digest("hex");
    const contractsForFile = allContracts.filter((c) => c.file === filePath);

    writeFileSync(contractPath, JSON.stringify({
      file_hash: fileHash,
      ...(principleHash ? { principle_hash: principleHash } : {}),
      contracts: contractsForFile,
    }, null, 2));
  }
}
