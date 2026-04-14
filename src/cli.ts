#!/usr/bin/env node

/**
 * neurallog CLI
 *
 * Five-phase pipeline, filesystem as the bus.
 */

import { statSync, readFileSync } from "fs";
import { resolve, dirname } from "path";
import {
  buildDependencyGraph,
  assembleContexts,
  deriveContracts,
  classifyPrinciples,
  applyAxiomsPhase,
} from "./phases";

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.length === 0 || args.includes("--help")) {
    printHelp();
    process.exit(0);
  }

  const command = args[0];
  const model = getFlag(args, "--model") || "sonnet";
  const verbose = args.includes("--verbose") || args.includes("-v");
  const dryRun = args.includes("--dry-run");

  switch (command) {
    case "analyze":
      await runAnalyze(args.slice(1), model, verbose, dryRun);
      break;
    case "verify":
      runVerify(args.slice(1));
      break;
    default:
      console.error(`Unknown command: ${command}`);
      printHelp();
      process.exit(1);
  }
}

async function runAnalyze(
  args: string[],
  model: string,
  verbose: boolean,
  dryRun: boolean
): Promise<void> {
  const filePath = require("path").resolve(args.find((a) => !a.startsWith("-")) || ".");
  const projectRoot = findProjectRoot(require("path").dirname(filePath));

  console.log("neurallog v0.2.0");
  console.log(`File:    ${filePath}`);
  console.log(`Model:   ${model}`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  // Phase 1: Dependency graph
  const graph = buildDependencyGraph(filePath, projectRoot);

  // Phase 2: Context assembly
  const bundles = assembleContexts(graph);

  if (bundles.length === 0) {
    console.log("No log statements found in the dependency graph.");
    process.exit(0);
  }

  // Phase 3: Contract derivation
  const derivation = await deriveContracts(bundles, projectRoot, model, verbose);

  // Phase 4: Principle classification
  await classifyPrinciples(derivation, projectRoot, model);

  // Phase 5: Axiom application
  const report = applyAxiomsPhase(projectRoot);

  // Summary
  console.log("═══════════════════════════════════════════════════════════");
  console.log(`  ${graph.files.length} files | ${derivation.contracts.length} contracts`);
  console.log(`  Phase 3: ${derivation.contracts.reduce((n, c) => n + c.proven.length, 0)} proven | ${derivation.contracts.reduce((n, c) => n + c.violations.length, 0)} violations`);
  console.log(`  Phase 5: ${report.proven} proven | ${report.violations} violations | ${report.consistency} consistency`);
  console.log("═══════════════════════════════════════════════════════════");

  // Issue filing
  if (args.includes("--issues") || dryRun) {
    const { collectViolationIssues, fileViolationIssues } = require("./issues");
    // Adapt derivation output to the old AnalysisResult format for now
    const fakeResults = derivation.contracts.map((c) => ({
      derivation: {
        callSite: { line: c.line, column: 0, logText: "", functionName: c.function, functionSource: "", functionStartLine: c.line, functionEndLine: c.line },
        filePath: c.file,
        rawResponse: "",
      },
      verifications: [
        ...c.proven.map((p) => ({ smt2: p.smt2, z3Result: "unsat" as const, principle: p.principle, error: undefined })),
        ...c.violations.map((v) => ({ smt2: v.smt2, z3Result: "sat" as const, principle: v.principle, error: undefined })),
      ],
    }));

    const issues = collectViolationIssues(fakeResults);
    if (issues.length > 0) {
      console.log(`\n${dryRun ? "[DRY RUN] " : ""}Filing ${issues.length} issues...`);
      const result = fileViolationIssues(issues, dryRun);
      console.log(`Issues: ${result.filed} ${dryRun ? "previewed" : "filed"}, ${result.skipped} skipped, ${result.errors} errors`);
    }
  }
}

function runVerify(args: string[]): void {
  const projectRoot = require("path").resolve(args.find((a) => !a.startsWith("-")) || ".");

  console.log("neurallog verify — Phase 5 only (no LLM)");
  console.log(`Project: ${projectRoot}`);
  console.log();

  applyAxiomsPhase(projectRoot);
}

function printHelp(): void {
  console.log("neurallog — a logger that fixes your code");
  console.log();
  console.log("Usage:");
  console.log("  neurallog analyze <file.ts>     Full pipeline (phases 1-5)");
  console.log("  neurallog verify [project-root]  Phase 5 only (no LLM, just Z3)");
  console.log();
  console.log("Options:");
  console.log("  --model <name>   LLM model (default: sonnet)");
  console.log("  --verbose, -v    Stream LLM reasoning");
  console.log("  --issues         File GitHub issues for violations");
  console.log("  --dry-run        Preview issues without filing");
}

function getFlag(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  return idx !== -1 ? args[idx + 1] : undefined;
}

function findProjectRoot(startDir: string): string {
  let dir = startDir;
  const { dirname: dn } = require("path");
  while (dir !== dn(dir)) {
    const candidates = [".neurallog", "package.json", ".git"];
    for (const c of candidates) {
      try {
        const { resolve: res } = require("path");
        if (statSync(res(dir, c)).isDirectory() || statSync(res(dir, c)).isFile()) {
          return dir;
        }
      } catch {
        continue;
      }
    }
    dir = dn(dir);
  }
  return startDir;
}

main().catch((err) => {
  console.error("Fatal:", err.message || err);
  process.exit(1);
});
