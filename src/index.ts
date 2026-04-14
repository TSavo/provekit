#!/usr/bin/env node

import { readFileSync, statSync } from "fs";
import { resolve, dirname } from "path";
import { parseFile, findLogStatements } from "./parser";
import { deriveContract } from "./derivation";
import { verifyAll } from "./verifier";
import { ContractStore } from "./contracts";
import { PrincipleStore, findNewViolations, classifyAndGeneralize } from "./principles";
import { reportResults, AnalysisResult } from "./reporter";

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.length === 0 || args.includes("--help")) {
    console.log("Usage: neurallog analyze <file.ts>");
    console.log();
    console.log("Analyzes a TypeScript file and derives formal invariants");
    console.log("from every log statement. Verifies them with Z3.");
    console.log("Contracts and principles accumulate in .neurallog/");
    process.exit(0);
  }

  const command = args[0];
  if (command !== "analyze") {
    console.error(`Unknown command: ${command}`);
    console.error("Usage: neurallog analyze <file.ts>");
    process.exit(1);
  }

  const filePath = resolve(args[1]!);
  const model = args.includes("--model")
    ? args[args.indexOf("--model") + 1]!
    : "sonnet";
  const verbose = args.includes("--verbose") || args.includes("-v");

  const projectRoot = findProjectRoot(dirname(filePath));

  console.log(`neurallog v0.1.0`);
  console.log(`Analyzing: ${filePath}`);
  console.log(`Model: ${model}`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  const source = readFileSync(filePath, "utf-8");
  const tree = parseFile(source);
  const callSites = findLogStatements(tree, source);

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
      site, source, filePath, model, contractStore, principleStore, verbose
    );

    const verifications = verifyAll(derivation.rawResponse);

    // Store contract and write to disk
    const contract = ContractStore.fromVerificationResults(
      filePath, site.functionName, site.line, verifications
    );
    contractStore.add(contract);
    contractStore.writeToDisk(filePath, source);

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
        principleStore.add(principle);
        console.log(`NEW PRINCIPLE: ${principle.id} — ${principle.name}`);
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
