import { writeFileSync } from "fs";
import { join } from "path";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { applyAxioms, checkConsistency, AxiomResult } from "../axiom-engine";
import { ContractStore, Contract } from "../contracts";
import { IgnoreFilter } from "../git";

export interface AxiomReport {
  contractCount: number;
  axiomChecks: number;
  proven: number;
  violations: number;
  errors: number;
  unverified: number;
  ignored: number;
  consistency: string;
  consistencyDetail: string;
  staleContracts: number;
  results: AxiomResult[];
  reportedAt: string;
}

export class AxiomPhase extends Phase<void, AxiomReport> {
  readonly name = "Axiom Application";
  readonly phaseNumber = 5;

  execute(_input: void, options: PhaseOptions): PhaseResult<AxiomReport> {
    this.log("Mechanical axiom application (no LLM, no network, pure Z3)...");

    const store = new ContractStore(options.projectRoot);
    const allContracts = store.getAll();
    this.detail(`Loaded ${allContracts.length} contracts from .neurallog/contracts/`);

    const ignoreFilter = new IgnoreFilter(options.projectRoot);
    const contracts: Contract[] = [];
    let ignored = 0;
    for (const c of allContracts) {
      if (ignoreFilter.isIgnored(c.file)) {
        ignored++;
      } else {
        contracts.push(c);
      }
    }
    if (ignored > 0) {
      this.detail(`${ignored} contracts ignored via .neurallogignore, ${contracts.length} active`);
    }

    if (contracts.length === 0) {
      this.detail("No contracts to verify.");
      console.log();
      const report: AxiomReport = {
        contractCount: 0, axiomChecks: 0, proven: 0, violations: 0,
        errors: 0, unverified: 0, ignored, consistency: "n/a", consistencyDetail: "",
        staleContracts: 0, results: [],
        reportedAt: new Date().toISOString(),
      };
      const outPath = join(options.projectRoot, ".neurallog", "report.json");
      writeFileSync(outPath, JSON.stringify(report, null, 2));
      return { data: report, writtenTo: outPath };
    }

    const results = applyAxioms(contracts);

    let proven = 0;
    let violations = 0;
    let errors = 0;

    for (const r of results) {
      if (r.verdict === "proven") {
        proven++;
        this.detail(`✓ [${r.axiom}] ${r.description}`);
      } else if (r.verdict === "violation") {
        violations++;
        this.detail(`✗ [${r.axiom}] ${r.description}`);
      } else {
        errors++;
        this.detail(`⚠ [${r.axiom}] ${r.description} -- ${r.error?.slice(0, 60)}`);
      }
    }

    console.log();

    const unverified = results.filter((r) => r.z3Result === "unknown").length;

    this.detail("Checking consistency...");
    const consistency = checkConsistency(contracts);
    const consistencyResult = consistency[0]?.verdict || "n/a";
    let consistencyDetail = "";
    for (const c of consistency) {
      if (c.verdict === "proven") {
        this.detail(`✓ ${c.description}`);
      } else if (c.verdict === "violation") {
        this.detail(`✗ INCONSISTENCY: ${c.description}`);
        consistencyDetail = c.error || c.smt2?.slice(0, 500) || c.description;
      } else {
        this.detail(`⚠ ${c.description} -- ${c.error?.slice(0, 120)}`);
        if (!consistencyDetail) consistencyDetail = c.error || "";
      }
    }

    this.detail("Checking dependency chain...");
    const stale = store.findStale();
    if (stale.length === 0) {
      this.detail("✓ All dependencies current");
    } else {
      for (const s of stale) {
        this.detail(`⚠ STALE: ${s.key}`);
      }
    }

    console.log();
    this.detail(`${contracts.length} contracts, ${results.length} checks`);
    this.detail(`${proven} proven | ${violations} violations | ${unverified} unverified | ${errors} errors${ignored > 0 ? ` | ${ignored} ignored` : ""}`);
    this.detail(`Consistency: ${consistencyResult} | Stale: ${stale.length}`);
    if (consistencyDetail) {
      this.detail(`Consistency detail: ${consistencyDetail.slice(0, 200)}`);
    }
    this.detail("No LLM was used.");
    console.log();

    const report: AxiomReport = {
      contractCount: contracts.length,
      axiomChecks: results.length,
      proven,
      violations,
      errors,
      unverified,
      ignored,
      consistency: consistencyResult,
      consistencyDetail,
      staleContracts: stale.length,
      results,
      reportedAt: new Date().toISOString(),
    };

    const outPath = join(options.projectRoot, ".neurallog", "report.json");
    writeFileSync(outPath, JSON.stringify(report, null, 2));

    return { data: report, writtenTo: outPath };
  }
}
