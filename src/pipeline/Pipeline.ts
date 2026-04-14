import { readFileSync, existsSync } from "fs";
import { resolve, relative } from "path";
import { SignalRegistry, computeSignalHash } from "../signals";
import { ContractStore } from "../contracts";
import { DependencyPhase, DependencyGraph } from "./DependencyPhase";
import { ContextPhase, ContextBundle, CallSiteContext } from "./ContextPhase";
import { DerivationPhase, DerivationOutput } from "./DerivationPhase";
import { ClassificationPhase, ClassificationOutput } from "./ClassificationPhase";
import { AxiomPhase, AxiomReport } from "./AxiomPhase";
import { PhaseOptions } from "./Phase";
import { parseFile } from "../parser";

export interface PipelineConfig {
  entryFilePath: string;
  projectRoot: string;
  model: string;
  verbose: boolean;
  changedFiles?: string[];
  signalRegistry?: SignalRegistry;
}

export interface PipelineResult {
  graph: DependencyGraph;
  bundles: ContextBundle[];
  derivation: DerivationOutput;
  classification: ClassificationOutput;
  report: AxiomReport;
}

export class Pipeline {
  private dependencyPhase = new DependencyPhase();
  private contextPhase = new ContextPhase();
  private derivationPhase = new DerivationPhase();
  private classificationPhase = new ClassificationPhase();
  private axiomPhase = new AxiomPhase();

  async runFull(config: PipelineConfig): Promise<PipelineResult> {
    const signalRegistry = config.signalRegistry || SignalRegistry.createDefault();
    const options: PhaseOptions = {
      projectRoot: config.projectRoot,
      verbose: config.verbose,
    };

    const { data: graph } = this.dependencyPhase.execute(
      { entryFilePath: config.entryFilePath, signalRegistry, changedFiles: config.changedFiles },
      options
    );

    const { data: bundles } = await this.contextPhase.execute(
      { graph, signalRegistry },
      options
    );

    if (bundles.length === 0) {
      console.log("No signals found in the dependency graph.");
      const emptyDerivation: DerivationOutput = { contracts: [], newViolations: [], derivedAt: new Date().toISOString() };
      const emptyClassification: ClassificationOutput = { discovered: 0, validated: 0, rejected: 0, classifiedAt: new Date().toISOString() };
      const { data: report } = this.axiomPhase.execute(undefined, options);
      return { graph, bundles, derivation: emptyDerivation, classification: emptyClassification, report };
    }

    const { data: derivation } = await this.derivationPhase.execute(
      { bundles, model: config.model },
      options
    );

    const { data: classification } = await this.classificationPhase.execute(
      { derivation, model: config.model },
      options
    );

    const { data: report } = this.axiomPhase.execute(undefined, options);

    return { graph, bundles, derivation, classification, report };
  }

  async runIncremental(config: PipelineConfig): Promise<PipelineResult> {
    const signalRegistry = config.signalRegistry || SignalRegistry.createDefault();
    const options: PhaseOptions = {
      projectRoot: config.projectRoot,
      verbose: config.verbose,
    };

    console.log("neurallog: incremental verification...");

    const changedFiles = config.changedFiles || [];
    if (changedFiles.length === 0) {
      console.log("  No changed files.");
      const { data: report } = this.axiomPhase.execute(undefined, options);
      return {
        graph: { root: "", projectRoot: config.projectRoot, files: [], topologicalOrder: [], builtAt: new Date().toISOString() },
        bundles: [],
        derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
        classification: { discovered: 0, validated: 0, rejected: 0, classifiedAt: new Date().toISOString() },
        report,
      };
    }

    const store = new ContractStore(config.projectRoot);
    const existingContracts = store.getAll();

    const staleContracts = new Set<string>();
    let freshCount = 0;

    for (const filePath of changedFiles) {
      if (!existsSync(filePath)) continue;

      const source = readFileSync(filePath, "utf-8");
      const tree = parseFile(source);
      const signals = signalRegistry.hasAsyncGenerators()
        ? await signalRegistry.findAllAsync(filePath, source, tree)
        : signalRegistry.findAll(filePath, source, tree);

      for (const signal of signals) {
        const currentHash = computeSignalHash(signal);
        const key = `${signal.file}:${signal.functionName}:${signal.line}`;

        const existing = existingContracts.find(
          (c) => c.file === filePath && c.function === signal.functionName && Math.abs(c.line - signal.line) <= 2
        );

        if (existing && existing.signal_hash === currentHash) {
          freshCount++;
        } else {
          staleContracts.add(key);
          if (existing) {
            console.log(`  STALE: ${relative(config.projectRoot, filePath)}:${signal.functionName}:${signal.line} — signal changed`);
          } else {
            console.log(`  NEW:   ${relative(config.projectRoot, filePath)}:${signal.functionName}:${signal.line} — no existing contract`);
          }
        }
      }
    }

    // Cascade: contracts that depend on stale contracts are also stale
    let cascaded = true;
    while (cascaded) {
      cascaded = false;
      for (const c of existingContracts) {
        const key = `${c.file}:${c.function}:${c.line}`;
        if (staleContracts.has(key)) continue;

        for (const depHash of c.depends_on) {
          const depContract = existingContracts.find(
            (d) => ContractStore.contractHash(d) === depHash
          );
          if (depContract) {
            const depKey = `${depContract.file}:${depContract.function}:${depContract.line}`;
            if (staleContracts.has(depKey)) {
              staleContracts.add(key);
              console.log(`  CASCADE: ${relative(config.projectRoot, c.file)}:${c.function}:${c.line} — depends on stale contract`);
              cascaded = true;
              break;
            }
          }
        }
      }
    }

    console.log(`  ${freshCount} fresh, ${staleContracts.size} stale`);

    if (staleContracts.size === 0) {
      console.log("  All contracts current. Running Phase 5...");
      console.log();
      const { data: report } = this.axiomPhase.execute(undefined, options);
      return {
        graph: { root: "", projectRoot: config.projectRoot, files: [], topologicalOrder: [], builtAt: new Date().toISOString() },
        bundles: [],
        derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
        classification: { discovered: 0, validated: 0, rejected: 0, classifiedAt: new Date().toISOString() },
        report,
      };
    }

    console.log(`  Re-deriving ${staleContracts.size} contracts...`);
    console.log();

    return this.runFull({
      ...config,
      changedFiles,
      signalRegistry,
    });
  }

  runVerifyOnly(projectRoot: string, verbose: boolean = false): AxiomReport {
    const options: PhaseOptions = { projectRoot, verbose };
    const { data: report } = this.axiomPhase.execute(undefined, options);
    return report;
  }
}
