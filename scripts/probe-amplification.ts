#!/usr/bin/env tsx
/**
 * Diagnostic: print how many mutations each curated base produces under the
 * Stryker amplifier, and (when verbose) the operator catalog hits per base.
 *
 * Usage: npx tsx scripts/probe-amplification.ts
 */

import { loadScenarios } from "../src/fix/corpus/index.js";
import { amplifyScenario } from "../src/fix/corpus/amplify-stryker.js";

function main(): void {
  const corpus = loadScenarios("all");
  const eligible = corpus.filter((s) => s.expected.outcome !== "out_of_scope");

  process.stdout.write(
    `Corpus: ${corpus.length} total, ${eligible.length} eligible (non-out_of_scope).\n\n`,
  );

  let totalAmplified = 0;
  for (const s of eligible) {
    const all = amplifyScenario(s, { maxMutations: 50, includeNonPreserving: true });
    const preserving = all.filter((a) => a.preservation === "preserves_bug");
    const removing = all.filter((a) => a.preservation === "removes_bug");
    const uncertain = all.filter((a) => a.preservation === "uncertain");
    process.stdout.write(
      `  ${s.id.padEnd(28)} class=${s.bugClass.padEnd(24)} preserves=${preserving.length} removes=${removing.length} uncertain=${uncertain.length}\n`,
    );
    for (const a of preserving) {
      process.stdout.write(`    + [${a.id}] ${a.mutationKind}\n`);
    }
    totalAmplified += preserving.length;
  }
  process.stdout.write(`\nTotal preserves_bug mutations across eligible bases: ${totalAmplified}\n`);
}

main();
