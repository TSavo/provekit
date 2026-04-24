import { writeFileSync } from "fs";
import { join } from "path";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { ContractStore, Contract } from "../contracts";
import { IgnoreFilter } from "../git";
import {
  Checker, CheckResult,
  ConsistencyChecker,
  EntailmentChecker,
  ReachabilityChecker,
  StrengtheningChecker,
  IndependenceChecker,
  TemplateChecker,
  StrengthChecker,
  PropertyTestChecker,
} from "../checkers";

export interface AxiomReport {
  contractCount: number;
  checkerResults: { checker: string; proven: number; violations: number; errors: number; results: CheckResult[] }[];
  totalProven: number;
  totalViolations: number;
  totalErrors: number;
  ignored: number;
  staleContracts: number;
  reportedAt: string;
}

export class AxiomPhase extends Phase<void, AxiomReport> {
  readonly name = "Axiom Application";
  readonly phaseNumber = 5;

  async execute(_input: void, options: PhaseOptions): Promise<PhaseResult<AxiomReport>> {
    this.log("Mechanical verification (no LLM, no network, pure Z3)...");

    const store = new ContractStore(options.projectRoot);
    const allContracts = store.getAll();
    this.detail(`Loaded ${allContracts.length} contracts from .provekit/contracts/`);

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
      this.detail(`${ignored} contracts ignored via .provekitignore, ${contracts.length} active`);
    }

    if (contracts.length === 0) {
      this.detail("No contracts to verify.");
      console.log();
      const report: AxiomReport = {
        contractCount: 0, checkerResults: [], totalProven: 0, totalViolations: 0,
        totalErrors: 0, ignored, staleContracts: 0, reportedAt: new Date().toISOString(),
      };
      const outPath = join(options.projectRoot, ".provekit", "report.json");
      writeFileSync(outPath, JSON.stringify(report, null, 2));
      return { data: report, writtenTo: outPath };
    }

    const callGraph = this.buildCallGraph(contracts);

    const checkers: Checker[] = [
      new TemplateChecker(),
      new ConsistencyChecker(),
      new EntailmentChecker(),
      new ReachabilityChecker(),
      new StrengtheningChecker(),
      new IndependenceChecker(),
      new StrengthChecker(),
      new PropertyTestChecker(options.projectRoot),
    ];

    const checkerResults: AxiomReport["checkerResults"] = [];
    let totalProven = 0;
    let totalViolations = 0;
    let totalErrors = 0;

    for (const checker of checkers) {
      const startTime = Date.now();
      console.log(`  [${checker.name}] running...`);

      const results = checker.check(contracts, callGraph);
      const elapsed = Date.now() - startTime;

      let proven = 0;
      let violations = 0;
      let errors = 0;

      for (const r of results) {
        if (r.verdict === "proven") {
          proven++;
        } else if (r.verdict === "violation") {
          violations++;
          this.detail(`  ✗ [${checker.name}] ${r.description}`);
        } else {
          errors++;
        }
      }

      console.log(`  [${checker.name}] ${this.formatDuration(elapsed)}: ${proven} proven, ${violations} violations, ${errors} errors`);

      totalProven += proven;
      totalViolations += violations;
      totalErrors += errors;

      checkerResults.push({ checker: checker.name, proven, violations, errors, results });
    }

    let judgeRan = false;
    let harnessRan = false;
    const pChecker = checkers.find((c) => c.name === "property-test") as PropertyTestChecker | undefined;
    if (pChecker && process.env.NEURALLOG_PROPERTY_TEST_JUDGE === "1") {
      const stats = await pChecker.judgeResults();
      if (stats.judged > 0) {
        judgeRan = true;
        this.detail(`property-test judge: ${stats.judged} judged (${stats.cacheHits} cache hits), ${stats.flipped} verdict flips, ${stats.confirmed} violations confirmed`);
      }
    }

    if (pChecker && process.env.NEURALLOG_HARNESS_SYNTHESIS === "1") {
      const stats = await pChecker.synthesizeAndRunHarnesses();
      if (stats.attempted > 0) {
        harnessRan = true;
        const pEntry = checkerResults.find((r) => r.checker === "property-test");
        if (pEntry) {
          pEntry.results.push(...pChecker.harnessResults);
        }
        this.detail(`harness: ${stats.attempted} attempted | ${stats.pass} pass | ${stats.encodingGap} encoding-gap | ${stats.harnessError} harness-error | ${stats.untestable} untestable | ${stats.timeout} timeout | ${stats.synthesisFailed} synth-failed`);
      }
    }

    if (judgeRan || harnessRan) {
      const pEntry = checkerResults.find((r) => r.checker === "property-test");
      if (pEntry) {
        pEntry.proven = 0;
        pEntry.violations = 0;
        pEntry.errors = 0;
        for (const r of pEntry.results) {
          if (r.verdict === "proven") pEntry.proven++;
          else if (r.verdict === "violation") pEntry.violations++;
          else pEntry.errors++;
        }
        totalProven = checkerResults.reduce((n, x) => n + x.proven, 0);
        totalViolations = checkerResults.reduce((n, x) => n + x.violations, 0);
        totalErrors = checkerResults.reduce((n, x) => n + x.errors, 0);
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
    this.detail(`${contracts.length} contracts, ${checkers.length} checkers`);
    this.detail(`${totalProven} proven | ${totalViolations} violations | ${totalErrors} errors${ignored > 0 ? ` | ${ignored} ignored` : ""}`);
    this.detail(`Stale: ${stale.length}`);
    this.detail(
      (judgeRan || harnessRan)
        ? `Property-test LLM usage: ${judgeRan ? "judge" : ""}${judgeRan && harnessRan ? " + " : ""}${harnessRan ? "harness synthesis" : ""}; all other checkers were mechanical.`
        : "No LLM was used."
    );
    console.log();

    const report: AxiomReport = {
      contractCount: contracts.length,
      checkerResults,
      totalProven,
      totalViolations,
      totalErrors,
      ignored,
      staleContracts: stale.length,
      reportedAt: new Date().toISOString(),
    };

    const outPath = join(options.projectRoot, ".provekit", "report.json");
    writeFileSync(outPath, JSON.stringify(report, null, 2));

    return { data: report, writtenTo: outPath };
  }

  private buildCallGraph(contracts: Contract[]): Map<string, string[]> {
    const graph = new Map<string, string[]>();
    for (const c of contracts) {
      const fnKey = `${c.file}/${c.function}`;
      if (!graph.has(fnKey)) graph.set(fnKey, []);
      for (const dep of c.depends_on) {
        const depParts = dep.match(/^(.+)\/([^/]+)\[\d+\]$/);
        if (depParts && depParts[2]) {
          const depFn = depParts[2];
          const fnList = graph.get(fnKey);
          if (fnList && !fnList.includes(depFn)) {
            fnList.push(depFn);
          }
        }
      }
    }
    return graph;
  }

  private formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s`;
    return `${Math.floor(s / 60)}m ${s % 60}s`;
  }
}
