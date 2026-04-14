#!/usr/bin/env node

import { readFileSync, statSync } from "fs";
import { resolve, dirname } from "path";
import { parseFile, findLogStatements } from "./parser";
import { deriveContract } from "./derivation";
import { verifyAll } from "./verifier";
import { ContractStore } from "./contracts";
import { PrincipleStore, findNewViolations, classifyAndGeneralize } from "./principles";
import { reportResults, AnalysisResult } from "./reporter";
import { collectViolationIssues, fileViolationIssues } from "./issues";
import { applyAxioms, checkConsistency } from "./axiom-engine";
import { findStaleContracts } from "./contracts";
import { resolveImports, ResolvedImport } from "./imports";

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.length === 0 || args.includes("--help")) {
    console.log("Usage: neurallog analyze <file.ts>");
    console.log();
    console.log("Analyzes a TypeScript file and derives formal invariants");
    console.log("from every log statement. Verifies them with Z3.");
    console.log("Contracts and principles accumulate in .neurallog/");
    console.log();
    console.log("Options:");
    console.log("  --model <name>  LLM model to use (default: sonnet)");
    console.log("  --verbose, -v   Show verbose output");
    console.log("  --issues        File GitHub issues for Z3-confirmed violations (sat)");
    console.log("  --dry-run       Print issues without filing them (use with --issues)");
    process.exit(0);
  }

  const command = args[0];
  if (command === "verify") {
    await runVerify(args.slice(1));
    return;
  }
  if (command !== "analyze") {
    console.error(`Unknown command: ${command}`);
    console.error("Usage: neurallog analyze <file.ts>");
    console.error("       neurallog verify <project-root>  (Layer 2: no LLM, just Z3)");
    process.exit(1);
  }

  const filePath = resolve(args[1]!);
  const model = args.includes("--model")
    ? args[args.indexOf("--model") + 1]!
    : "sonnet";
  const verbose = args.includes("--verbose") || args.includes("-v");
  const fileIssues = args.includes("--issues");
  const dryRun = args.includes("--dry-run");

  const projectRoot = findProjectRoot(dirname(filePath));

  console.log(`neurallog v0.1.0`);
  console.log(`Analyzing: ${filePath}`);
  console.log(`Model: ${model}`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  const source = readFileSync(filePath, "utf-8");
  const tree = parseFile(source);
  const callSites = findLogStatements(tree, source);

  // Phase -1: Build dependency graph
  const imports = resolveImports(tree, filePath);
  if (imports.length > 0) {
    console.log(`Resolved ${imports.length} import${imports.length === 1 ? "" : "s"}:`);
    for (const imp of imports) {
      console.log(`  ${imp.specifier} → ${imp.resolvedPath}`);
    }
    console.log();
  }

  console.log(
    `Found ${callSites.length} log statement${callSites.length === 1 ? "" : "s"}`
  );

  if (callSites.length === 0) {
    console.log("Nothing to analyze.");
    process.exit(0);
  }

  for (const site of callSites) {
    console.log(
      `  ${site.line}:${site.column}  ${site.functionName}  ${site.logText.slice(0, 60)}...`
    );
  }

  // Load existing state from .neurallog/
  const contractStore = new ContractStore(projectRoot);
  const principleStore = new PrincipleStore(projectRoot);

  const existingContracts = contractStore.getAll().length;
  const existingPrinciples = principleStore.getAll().length;
  if (existingContracts > 0 || existingPrinciples > 0) {
    console.log(
      `\nLoaded: ${existingContracts} contracts, ${existingPrinciples} discovered principles`
    );
  }

  console.log();
  console.log("Phase 1: Deriving contracts and verifying with Z3...");
  console.log();

  const results: AnalysisResult[] = [];
  const allNewViolations: { violation: any; context: string }[] = [];

  for (const site of callSites) {
    process.stdout.write(
      `  ${site.functionName}:${site.line} (${contractStore.getAll().length} contracts) ... `
    );

    const derivation = await deriveContract(
      site, source, filePath, model, contractStore, principleStore, verbose, imports
    );

    const verifications = verifyAll(derivation.rawResponse);

    // Store contract with dependency tracking and write to disk
    const contract = ContractStore.fromVerificationResults(
      filePath, site.functionName, site.line, verifications
    );
    ContractStore.withDependencies(contract, contractStore.getAll());
    contractStore.add(contract);
    contractStore.writeToDisk(filePath, source, principleStore.computePrincipleHash());

    // Collect [NEW] violations for Phase 2
    const newViolations = findNewViolations(
      verifications, filePath, site.functionName, site.line
    );
    allNewViolations.push(...newViolations);

    const proven = verifications.filter((v) => v.z3Result === "unsat").length;
    const violations = verifications.filter((v) => v.z3Result === "sat").length;
    const newCount = newViolations.length;

    console.log(
      `${verifications.length} blocks: ${proven} proven, ${violations} violations` +
        (newCount > 0 ? ` (${newCount} [NEW])` : "")
    );

    results.push({ derivation, verifications });
  }

  // Phase 2: Classify [NEW] violations and potentially grow principles
  if (allNewViolations.length > 0) {
    console.log();
    console.log(
      `Phase 2: ${allNewViolations.length} [NEW] violation${allNewViolations.length === 1 ? "" : "s"} found. Classifying...`
    );

    for (const { violation, context } of allNewViolations) {
      process.stdout.write(`  Analyzing ${context} ... `);

      const principle = await classifyAndGeneralize(
        violation, context, principleStore.getAll(), model
      );

      if (principle) {
        principle.id = principleStore.nextId();
        const tag = principle.validated ? "VALIDATED" : "UNVALIDATED";
        if (principle.validated) {
          principleStore.add(principle);
          console.log(`NEW PRINCIPLE [${tag}]: ${principle.id} — ${principle.name}`);
        } else {
          console.log(`REJECTED PRINCIPLE [${tag}]: ${principle.id} — ${principle.name}`);
          if (principle.validationFailure) {
            console.log(`    Reason: ${principle.validationFailure}`);
          }
        }
      } else {
        console.log("mapped to existing principle");
      }
    }

    const newPrincipleCount =
      principleStore.getAll().length - existingPrinciples;
    if (newPrincipleCount > 0) {
      console.log(
        `\n${newPrincipleCount} new principle${newPrincipleCount === 1 ? "" : "s"} discovered and saved to .neurallog/principles/`
      );
    }
  }

  reportResults(results);

  // File GitHub issues for sat violations if --issues flag is set
  if (fileIssues || dryRun) {
    const issues = collectViolationIssues(results);

    if (issues.length === 0) {
      console.log("\nNo Z3-confirmed violations to file as issues.");
    } else {
      console.log(
        `\n${dryRun ? "[DRY RUN] " : ""}Filing ${issues.length} GitHub issue${issues.length === 1 ? "" : "s"} for Z3-confirmed violations...`
      );
      const { filed, skipped, errors } = fileViolationIssues(issues, dryRun);
      console.log(
        `\nIssues: ${filed} ${dryRun ? "previewed" : "filed"}, ${skipped} skipped (duplicate), ${errors} errors`
      );
    }
  }
}

async function runVerify(args: string[]): Promise<void> {
  const projectRoot = resolve(args[0] || ".");

  console.log("neurallog verify — Layer 2: mechanical axiom application");
  console.log(`Project: ${projectRoot}`);
  console.log("No LLM. No network. Just Z3 against cached contracts.");
  console.log();

  const store = new ContractStore(projectRoot);
  const contracts = store.getAll();

  if (contracts.length === 0) {
    console.log("No contracts found in .neurallog/. Run 'neurallog analyze' first.");
    process.exit(0);
  }

  console.log(`Loaded ${contracts.length} contracts from .neurallog/`);
  console.log();

  // Apply all axiom templates
  console.log("Applying axiom templates...");
  const results = applyAxioms(contracts);

  let proven = 0;
  let violations = 0;
  let errors = 0;

  for (const r of results) {
    if (r.verdict === "proven") {
      proven++;
      console.log(`  ✓ [${r.axiom}] ${r.description}`);
    } else if (r.verdict === "violation") {
      violations++;
      console.log(`  ✗ [${r.axiom}] ${r.description}`);
    } else {
      errors++;
      console.log(`  ⚠ [${r.axiom}] ${r.description} — ${r.error?.slice(0, 60)}`);
    }
  }

  console.log();

  // Cross-contract consistency
  console.log("Checking cross-contract consistency...");
  const consistency = checkConsistency(contracts);
  for (const c of consistency) {
    if (c.verdict === "proven") {
      console.log(`  ✓ ${c.description}`);
    } else if (c.verdict === "violation") {
      console.log(`  ✗ INCONSISTENCY: ${c.description}`);
    } else {
      console.log(`  ⚠ ${c.description} — ${c.error?.slice(0, 60)}`);
    }
  }

  // Dependency chain staleness
  console.log();
  console.log("Checking dependency chain...");
  const stale = findStaleContracts(contracts);
  if (stale.length === 0) {
    console.log("  ✓ All dependencies current");
  } else {
    for (const s of stale) {
      console.log(`  ⚠ STALE: ${s.function}:${s.line} — upstream dependency changed, needs re-derivation`);
    }
  }

  console.log();
  console.log("═══════════════════════════════════════════════════════════");
  console.log(`  ${contracts.length} contracts, ${results.length} axiom checks`);
  console.log(`  ${proven} proven  |  ${violations} violations  |  ${errors} errors`);
  console.log(`  Consistency: ${consistency[0]?.verdict || "n/a"}`);
  console.log(`  Dependencies: ${stale.length === 0 ? "all current" : `${stale.length} stale`}`);
  console.log("  No LLM was used.");
  console.log("═══════════════════════════════════════════════════════════");
}

function findProjectRoot(startDir: string): string {
  let dir = startDir;
  while (dir !== dirname(dir)) {
    const candidates = [".neurallog", "package.json", ".git"];
    for (const c of candidates) {
      try {
        if (statSync(resolve(dir, c)).isDirectory() || statSync(resolve(dir, c)).isFile()) {
          return dir;
        }
      } catch {
        continue;
      }
    }
    dir = dirname(dir);
  }
  return startDir;
}

main().catch((err) => {
  console.error("Fatal:", err.message || err);
  process.exit(1);
});
