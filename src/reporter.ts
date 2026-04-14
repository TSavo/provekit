import { DerivationResult } from "./derivation";
import { VerificationResult } from "./verifier";

export interface AnalysisResult {
  derivation: DerivationResult;
  verifications: VerificationResult[];
}

export function reportResults(results: AnalysisResult[]): void {
  console.log();
  console.log("═══════════════════════════════════════════════════════════");
  console.log("  neurallog analysis report");
  console.log("═══════════════════════════════════════════════════════════");

  let totalProven = 0;
  let totalSat = 0;
  let totalError = 0;
  let totalBlocks = 0;

  for (const { derivation, verifications } of results) {
    console.log();
    console.log(
      `─── ${derivation.filePath}:${derivation.callSite.line} ` +
        `[${derivation.callSite.functionName}] ───`
    );
    console.log(
      `Log: ${derivation.callSite.logText.slice(0, 80)}`
    );
    console.log();

    if (verifications.length === 0) {
      console.log("  (no SMT-LIB blocks extracted)");
      continue;
    }

    for (const v of verifications) {
      totalBlocks++;
      const tag = v.principle ? `[${v.principle}]` : "[?]";
      if (v.z3Result === "unsat") {
        totalProven++;
        const trivialTag = v.trivial ? " [trivial identity]" : "";
        console.log(`  ✓ PROVEN (unsat)  ${tag}${trivialTag}`);
      } else if (v.z3Result === "sat") {
        totalSat++;
        console.log(`  ✗ VIOLATION REACHABLE (sat)  ${tag}`);
      } else if (v.z3Result === "error") {
        totalError++;
        console.log(`  ⚠ Z3 ERROR  ${tag}  ${v.error?.slice(0, 60) || ""}`);
      } else {
        console.log(`  ? UNKNOWN  ${tag}`);
      }

      // Show first few lines of the SMT-LIB for context
      const lines = v.smt2.split("\n").filter(l => l.startsWith(";"));
      for (const line of lines.slice(0, 3)) {
        console.log(`    ${line}`);
      }
    }
  }

  console.log();
  console.log("═══════════════════════════════════════════════════════════");
  console.log(`  ${results.length} log statements analyzed`);
  console.log(`  ${totalBlocks} SMT-LIB blocks verified by Z3`);
  console.log(`  ${totalProven} proven (unsat)  |  ${totalSat} violations (sat)  |  ${totalError} errors`);
  console.log("═══════════════════════════════════════════════════════════");
}
