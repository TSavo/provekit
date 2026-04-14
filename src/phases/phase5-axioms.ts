/**
 * Phase 5: Axiom Application
 *
 * Input:  .neurallog/contracts/*.json (from Phase 3)
 *         .neurallog/principles/*.json (from Phase 4)
 * Output: .neurallog/report.json (immutable)
 *
 * Mechanical axiom application against cached contracts.
 * No LLM. No network. Pure Z3. Cross-contract consistency.
 * Dependency chain staleness detection.
 */

import { writeFileSync, readFileSync } from "fs";
import { join } from "path";
import { applyAxioms, checkConsistency, AxiomResult } from "../axiom-engine";
import { ContractStore, findStaleContracts } from "../contracts";

export interface AxiomReport {
  contractCount: number;
  axiomChecks: number;
  proven: number;
  violations: number;
  errors: number;
  consistency: string;
  staleContracts: number;
  results: AxiomResult[];
  reportedAt: string;
}

export function applyAxiomsPhase(projectRoot: string): AxiomReport {
  console.log("Phase 5: Mechanical axiom application (no LLM, no network, pure Z3)...");

  const store = new ContractStore(projectRoot);
  const contracts = store.getAll();
  console.log(`  Loaded ${contracts.length} contracts from .neurallog/contracts/`);

  if (contracts.length === 0) {
    console.log("  No contracts to verify.");
    console.log();
    return {
      contractCount: 0, axiomChecks: 0, proven: 0, violations: 0,
      errors: 0, consistency: "n/a", staleContracts: 0, results: [],
      reportedAt: new Date().toISOString(),
    };
  }

  // Apply axiom templates
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
  console.log("  Checking consistency...");
  const consistency = checkConsistency(contracts);
  const consistencyResult = consistency[0]?.verdict || "n/a";
  for (const c of consistency) {
    if (c.verdict === "proven") console.log(`  ✓ ${c.description}`);
    else if (c.verdict === "violation") console.log(`  ✗ INCONSISTENCY: ${c.description}`);
    else console.log(`  ⚠ ${c.description} — ${c.error?.slice(0, 60)}`);
  }

  // Dependency staleness
  console.log("  Checking dependency chain...");
  const stale = findStaleContracts(contracts);
  if (stale.length === 0) {
    console.log("  ✓ All dependencies current");
  } else {
    for (const s of stale) {
      console.log(`  ⚠ STALE: ${s.function}:${s.line}`);
    }
  }

  console.log();
  console.log(`  ${contracts.length} contracts, ${results.length} checks`);
  console.log(`  ${proven} proven | ${violations} violations | ${errors} errors`);
  console.log(`  Consistency: ${consistencyResult} | Stale: ${stale.length}`);
  console.log("  No LLM was used.");
  console.log();

  const report: AxiomReport = {
    contractCount: contracts.length,
    axiomChecks: results.length,
    proven,
    violations,
    errors,
    consistency: consistencyResult,
    staleContracts: stale.length,
    results,
    reportedAt: new Date().toISOString(),
  };

  writeFileSync(
    join(projectRoot, ".neurallog", "report.json"),
    JSON.stringify(report, null, 2)
  );

  return report;
}
