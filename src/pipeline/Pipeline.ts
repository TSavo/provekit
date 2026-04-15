import { readFileSync, existsSync } from "fs";
import { resolve, relative } from "path";
import { SignalRegistry, computeSignalHash } from "../signals";
import { ContractStore, Contract, signalKey, contractHash } from "../contracts";
import { PrincipleStore, hashPrinciple } from "../principles";
import { DependencyPhase, DependencyGraph } from "./DependencyPhase";
import { ContextPhase, ContextBundle, CallSiteContext } from "./ContextPhase";
import { DerivationPhase, DerivationOutput } from "./DerivationPhase";
import { AxiomPhase, AxiomReport } from "./AxiomPhase";
import { PhaseOptions } from "./Phase";
import { parseFile } from "../parser";

export interface PipelineConfig {
  entryFilePath: string;
  projectRoot: string;
  model: string;
  verbose: boolean;
  maxConcurrency?: number;
  changedFiles?: string[];
  signalRegistry?: SignalRegistry;
  provider?: import("../llm").LLMProvider;
}

export interface PipelineResult {
  graph: DependencyGraph;
  bundles: ContextBundle[];
  derivation: DerivationOutput;
  report: AxiomReport;
}

export class Pipeline {
  private dependencyPhase = new DependencyPhase();
  private contextPhase = new ContextPhase();
  private derivationPhase = new DerivationPhase();
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
      const { data: report } = this.axiomPhase.execute(undefined, options);
      return { graph, bundles, derivation: emptyDerivation, report };
    }

    const store = new ContractStore(config.projectRoot);
    const staleBundles = this.filterStaleBundles(bundles, store);

    if (staleBundles.length === 0) {
      console.log("All signal hashes current. No derivation needed.");
      const emptyDerivation: DerivationOutput = { contracts: [], newViolations: [], derivedAt: new Date().toISOString() };
      const { data: report } = this.axiomPhase.execute(undefined, options);
      return { graph, bundles, derivation: emptyDerivation, report };
    }

    const staleSiteCount = staleBundles.reduce((n, b) => n + b.callSites.length, 0);
    const skippedBundles = bundles.length - staleBundles.length;
    const skippedSites = bundles.reduce((n, b) => n + b.callSites.length, 0) - staleSiteCount;
    if (skippedBundles > 0) {
      console.log(`  ${skippedBundles} files skipped (signal hashes current), ${skippedSites} signals already derived`);
      console.log(`  ${staleBundles.length} files need derivation, ${staleSiteCount} signals`);
      console.log();
    }

    const { data: derivation } = await this.derivationPhase.execute(
      { bundles: staleBundles, model: config.model, maxConcurrency: config.maxConcurrency, provider: config.provider },
      options
    );

    const { data: report } = this.axiomPhase.execute(undefined, options);

    return { graph, bundles, derivation, report };
  }

  async runIncremental(config: PipelineConfig): Promise<PipelineResult> {
    const signalRegistry = config.signalRegistry || SignalRegistry.createDefault();
    const options: PhaseOptions = {
      projectRoot: config.projectRoot,
      verbose: config.verbose,
    };

    const startTime = Date.now();
    console.log("neurallog: incremental verification...");
    console.log(`  Signals: ${signalRegistry.getGeneratorNames().join(", ")}`);

    const changedFiles = config.changedFiles || [];
    if (changedFiles.length === 0) {
      console.log("  No changed TypeScript files in staging area.");
      console.log("  Running Phase 5 against cached contracts...");
      const { data: report } = this.axiomPhase.execute(undefined, options);
      console.log(`  Completed in ${Date.now() - startTime}ms`);
      return {
        graph: { root: "", projectRoot: config.projectRoot, files: [], topologicalOrder: [], parallelGroups: [], builtAt: new Date().toISOString() },
        bundles: [],
        derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
                report,
      };
    }

    console.log(`  Changed files: ${changedFiles.length}`);
    for (const f of changedFiles) {
      console.log(`    ${relative(config.projectRoot, f)}`);
    }

    const store = new ContractStore(config.projectRoot);
    const existingContracts = store.getAll();
    const principleStore = new PrincipleStore(config.projectRoot);

    console.log(`  Existing contracts: ${existingContracts.length}`);
    console.log(`  Principles: ${principleStore.getPrincipleCount()} (7 seed + ${principleStore.getAll().length} discovered)`);
    console.log(`  Checking signal hashes...`);

    const staleContracts = new Set<string>();
    let freshCount = 0;
    let staleProofCount = 0;

    // Pass 1: signal-level staleness
    for (const filePath of changedFiles) {
      if (!existsSync(filePath)) continue;

      const source = readFileSync(filePath, "utf-8");
      const tree = parseFile(source);
      const signals = signalRegistry.hasAsyncGenerators()
        ? await signalRegistry.findAllAsync(filePath, source, tree)
        : signalRegistry.findAll(filePath, source, tree);

      const relPath = relative(config.projectRoot, filePath);
      for (const signal of signals) {
        const currentHash = computeSignalHash(signal);
        const key = signalKey(relPath, signal.functionName, signal.line);
        const existing = store.get(key);

        if (existing && existing.signal_hash === currentHash) {
          freshCount++;
        } else {
          staleContracts.add(key);
          console.log(`  ${existing ? "STALE" : "NEW"}:   ${key}`);
        }
      }
    }

    // Pass 2: principle-level staleness
    for (const c of existingContracts) {
      if (staleContracts.has(c.key)) continue;
      for (const proof of [...c.proven, ...c.violations]) {
        if (!proof.principle || !proof.principle_hash) continue;
        const currentPHash = principleStore.hashForPrinciple(proof.principle);
        if (currentPHash && proof.principle_hash !== currentPHash) {
          staleContracts.add(c.key);
          staleProofCount++;
          console.log(`  STALE: ${c.key} — principle ${proof.principle} changed`);
          break;
        }
      }
    }

    // Pass 3: cascade through depends_on (signal keys)
    let cascaded = true;
    while (cascaded) {
      cascaded = false;
      for (const c of existingContracts) {
        if (staleContracts.has(c.key)) continue;
        for (const dep of c.depends_on) {
          if (staleContracts.has(dep)) {
            staleContracts.add(c.key);
            console.log(`  CASCADE: ${c.key} — depends on ${dep}`);
            cascaded = true;
            break;
          }
        }
      }
    }

    if (staleProofCount > 0) {
      console.log(`  ${staleProofCount} proofs stale due to principle changes`);
    }

    console.log(`  ${freshCount} fresh, ${staleContracts.size} stale`);

    if (staleContracts.size === 0) {
      console.log("  All contracts current. Running Phase 5...");
      console.log();
      const { data: report } = this.axiomPhase.execute(undefined, options);
      console.log(`  Completed in ${formatDuration(Date.now() - startTime)}`);
      return {
        graph: { root: "", projectRoot: config.projectRoot, files: [], topologicalOrder: [], parallelGroups: [], builtAt: new Date().toISOString() },
        bundles: [],
        derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
                report,
      };
    }

    console.log(`  Re-deriving ${staleContracts.size} contracts...`);
    console.log();

    const result = await this.runFull({
      ...config,
      changedFiles,
      signalRegistry,
    });

    console.log(`  Incremental verification completed in ${formatDuration(Date.now() - startTime)}`);
    return result;
  }

  private filterStaleBundles(bundles: ContextBundle[], store: ContractStore): ContextBundle[] {
    const filtered: ContextBundle[] = [];

    for (const bundle of bundles) {
      const staleSites = bundle.callSites.filter((callSite) => {
        const key = signalKey(bundle.relativePath, callSite.functionName, callSite.line);
        const existing = store.get(key);
        return !existing || existing.signal_hash !== callSite.signalHash;
      });

      if (staleSites.length > 0) {
        filtered.push({ ...bundle, callSites: staleSites });
      }
    }

    return filtered;
  }

  runVerifyOnly(projectRoot: string, verbose: boolean = false): AxiomReport {
    const options: PhaseOptions = { projectRoot, verbose };
    const { data: report } = this.axiomPhase.execute(undefined, options);
    return report;
  }
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes < 60) return `${minutes}m ${remainingSeconds}s`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return `${hours}h ${remainingMinutes}m`;
}
