/**
 * Phase 2: Context Assembly
 *
 * Input:  .neurallog/graph.json (from Phase 1)
 * Output: .neurallog/contexts/*.json (immutable)
 *
 * For each file in topological order, assembles the context bundle:
 * source code, import sources, existing contracts, selected axioms.
 * Each bundle is everything the LLM needs for one derivation call.
 */

import { readFileSync, writeFileSync, mkdirSync } from "fs";
import { join, relative } from "path";
import { DependencyGraph, FileNode } from "./phase1-dependencies";
import { parseFile, findLogStatements, LogCallSite } from "../parser";

export interface ContextBundle {
  filePath: string;
  relativePath: string;
  callSites: CallSiteContext[];
  builtAt: string;
}

export interface CallSiteContext {
  line: number;
  column: number;
  functionName: string;
  logText: string;
  functionSource: string;
  fileSource: string;
  importSources: { path: string; source: string }[];
  existingContracts: string;  // formatted for prompt injection
  callingContext: string;
}

export function assembleContexts(graph: DependencyGraph): ContextBundle[] {
  console.log("Phase 2: Assembling context bundles...");
  console.log(`  Processing ${graph.topologicalOrder.length} files in dependency order`);

  const bundles: ContextBundle[] = [];

  for (const filePath of graph.topologicalOrder) {
    const fileNode = graph.files.find((f) => f.path === filePath);
    if (!fileNode) {
      console.log(`  WARNING: ${filePath} in topological order but not in graph`);
      continue;
    }
    if (fileNode.logStatements === 0) {
      console.log(`  ${fileNode.relativePath}: no log statements, skipping`);
      continue;
    }

    const source = readFileSync(filePath, "utf-8");
    const tree = parseFile(source);
    const callSites = findLogStatements(tree, source);

    if (callSites.length === 0) continue;

    // Gather import sources for this file
    const importSources: { path: string; source: string }[] = [];
    for (const impPath of fileNode.imports) {
      try {
        const impSource = readFileSync(impPath, "utf-8");
        importSources.push({
          path: relative(graph.projectRoot, impPath),
          source: impSource,
        });
      } catch {
        // skip unreadable imports
      }
    }

    // Load existing contracts for dependencies (from prior runs or earlier in this run)
    const existingContracts = loadExistingContracts(graph.projectRoot, fileNode.imports);

    const callSiteContexts: CallSiteContext[] = callSites.map((site) => {
      const isExported = site.functionSource.includes("export ");
      const visibility = isExported ? "public (exported)" : "module-private";

      return {
        line: site.line,
        column: site.column,
        functionName: site.functionName,
        logText: site.logText,
        functionSource: site.functionSource,
        fileSource: source,
        importSources,
        existingContracts,
        callingContext: `${site.functionName} is ${visibility}. ${
          isExported
            ? "Any caller can pass any arguments."
            : "Only called within this module."
        }`,
      };
    });

    const bundle: ContextBundle = {
      filePath,
      relativePath: relative(graph.projectRoot, filePath),
      callSites: callSiteContexts,
      builtAt: new Date().toISOString(),
    };

    bundles.push(bundle);

    const importContractCount = existingContracts === "(no existing contracts for imports)" ? 0 : existingContracts.split("###").length - 1;
    console.log(
      `  ${bundle.relativePath}: ${callSiteContexts.length} call sites, ${importSources.length} imports, ${importContractCount} dependency contracts`
    );
  }

  const outDir = join(graph.projectRoot, ".neurallog", "contexts");
  mkdirSync(outDir, { recursive: true });
  const bundlePath = join(outDir, "bundles.json");
  writeFileSync(bundlePath, JSON.stringify(bundles, null, 2));

  const totalCallSites = bundles.reduce((n, b) => n + b.callSites.length, 0);
  console.log(`  ${bundles.length} bundles, ${totalCallSites} total call sites`);
  console.log(`  Written to ${relative(graph.projectRoot, bundlePath)}`);
  console.log();

  return bundles;
}

function loadExistingContracts(projectRoot: string, importPaths: string[]): string {
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
    } catch {
      // No contracts for this import yet
    }
  }

  return sections.length > 0
    ? sections.join("\n\n")
    : "(no existing contracts for imports)";
}
