import { readFileSync, writeFileSync, mkdirSync } from "fs";
import { join, relative } from "path";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { DependencyGraph } from "./DependencyPhase";
import { SignalRegistry, Signal, computeSignalHash } from "../signals";
import { parseFile } from "../parser";

export interface CallSiteContext {
  line: number;
  column: number;
  functionName: string;
  signalText: string;
  signalType: string;
  signalHash: string;
  functionSource: string;
  fileSource: string;
  importSources: { path: string; source: string }[];
  existingContracts: string;
  callingContext: string;
  typeContext: string;
  pathConditions: string[];
}

export interface ContextBundle {
  filePath: string;
  relativePath: string;
  callSites: CallSiteContext[];
  builtAt: string;
}

export interface ContextInput {
  graph: DependencyGraph;
  signalRegistry: SignalRegistry;
}

export class ContextPhase extends Phase<ContextInput, ContextBundle[]> {
  readonly name = "Context Assembly";
  readonly phaseNumber = 2;

  async execute(input: ContextInput, options: PhaseOptions): Promise<PhaseResult<ContextBundle[]>> {
    const { graph, signalRegistry } = input;

    this.log("Assembling context bundles...");
    this.detail(`Processing ${graph.topologicalOrder.length} files in dependency order`);

    const bundles: ContextBundle[] = [];

    for (const filePath of graph.topologicalOrder) {
      const fileNode = graph.files.find((f) => f.path === filePath);
      if (!fileNode) continue;
      if (fileNode.signalCount === 0) {
        this.detail(`${fileNode.relativePath}: no signals, skipping`);
        continue;
      }

      const source = readFileSync(filePath, "utf-8");
      const tree = parseFile(source);
      const signals = signalRegistry.hasAsyncGenerators()
        ? await signalRegistry.findAllAsync(filePath, source, tree)
        : signalRegistry.findAll(filePath, source, tree);

      if (signals.length === 0) continue;

      const importSources = this.gatherImportSources(fileNode, graph.projectRoot);
      const existingContracts = this.loadExistingContracts(options.projectRoot, fileNode.imports);

      const callSiteContexts = signals.map((signal) =>
        this.buildCallSiteContext(signal, source, importSources, existingContracts)
      );

      const bundle: ContextBundle = {
        filePath,
        relativePath: relative(graph.projectRoot, filePath),
        callSites: callSiteContexts,
        builtAt: new Date().toISOString(),
      };

      bundles.push(bundle);

      const importContractCount = existingContracts === "(no existing contracts for imports)" ? 0 : existingContracts.split("###").length - 1;
      this.detail(
        `${bundle.relativePath}: ${callSiteContexts.length} signals, ${importSources.length} imports, ${importContractCount} dependency contracts`
      );
    }

    const outDir = join(options.projectRoot, ".neurallog", "contexts");
    mkdirSync(outDir, { recursive: true });
    const bundlePath = join(outDir, "bundles.json");
    writeFileSync(bundlePath, JSON.stringify(bundles, null, 2));

    const totalCallSites = bundles.reduce((n, b) => n + b.callSites.length, 0);
    this.detail(`${bundles.length} bundles, ${totalCallSites} total call sites`);
    this.detail(`Written to ${relative(options.projectRoot, bundlePath)}`);
    console.log();

    return { data: bundles, writtenTo: bundlePath };
  }

  private buildCallSiteContext(
    signal: Signal,
    source: string,
    importSources: { path: string; source: string }[],
    existingContracts: string
  ): CallSiteContext {
    const isExported = signal.functionSource.includes("export ");
    const visibility = isExported ? "public (exported)" : "module-private";

    const typeLines: string[] = [];
    if (signal.parameters.length > 0) {
      typeLines.push("Parameters:");
      for (const p of signal.parameters) {
        typeLines.push(`  ${p.name}: ${p.type}`);
      }
    }
    if (signal.returnType !== "unknown") {
      typeLines.push(`Return type: ${signal.returnType}`);
    }
    const localEntries = Object.entries(signal.localTypes);
    if (localEntries.length > 0) {
      typeLines.push("Local variables (before this signal):");
      for (const [name, type] of localEntries) {
        typeLines.push(`  ${name}: ${type}`);
      }
    }

    return {
      line: signal.line,
      column: signal.column,
      functionName: signal.functionName,
      signalText: signal.text,
      signalType: signal.type,
      signalHash: computeSignalHash(signal),
      functionSource: signal.functionSource,
      fileSource: source,
      importSources,
      existingContracts,
      callingContext: `${signal.functionName} is ${visibility}. ${
        isExported ? "Any caller can pass any arguments." : "Only called within this module."
      }`,
      typeContext: typeLines.length > 0 ? typeLines.join("\n") : "(no type annotations found)",
      pathConditions: signal.pathConditions,
    };
  }

  private gatherImportSources(
    fileNode: { imports: string[] },
    projectRoot: string
  ): { path: string; source: string }[] {
    const sources: { path: string; source: string }[] = [];
    for (const impPath of fileNode.imports) {
      try {
        const impSource = readFileSync(impPath, "utf-8");
        sources.push({ path: relative(projectRoot, impPath), source: impSource });
      } catch { /* skip unreadable */ }
    }
    return sources;
  }

  private loadExistingContracts(projectRoot: string, importPaths: string[]): string {
    const sections: string[] = [];

    for (const impPath of importPaths) {
      const relPath = relative(projectRoot, impPath);
      const contractPath = join(projectRoot, ".neurallog", "contracts", relPath + ".json");

      try {
        const data = JSON.parse(readFileSync(contractPath, "utf-8"));
        for (const contract of data.contracts) {
          const lines: string[] = [];
          lines.push(`### ${contract.file}:${contract.function} (line ${contract.line})`);

          if (contract.proven?.length > 0) {
            lines.push("\nProven properties (Z3 confirmed unsat):");
            for (const p of contract.proven) {
              const tag = p.principle ? `[${p.principle}]` : "";
              lines.push(`  ${tag} ${p.claim}`);
            }
          }

          if (contract.violations?.length > 0) {
            lines.push("\nKnown violations (Z3 confirmed sat):");
            for (const v of contract.violations) {
              const tag = v.principle ? `[${v.principle}]` : "";
              lines.push(`  ${tag} ${v.claim}`);
            }
          }

          sections.push(lines.join("\n"));
        }
      } catch { /* no contracts yet */ }
    }

    return sections.length > 0
      ? sections.join("\n\n")
      : "(no existing contracts for imports)";
  }
}
