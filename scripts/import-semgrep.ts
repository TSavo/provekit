#!/usr/bin/env tsx
/**
 * scripts/import-semgrep.ts — generate corpus scenarios from a local clone of
 * github.com/semgrep/semgrep-rules.
 *
 * Usage:
 *   npx tsx scripts/import-semgrep.ts <path-to-semgrep-rules>
 *   npx tsx scripts/import-semgrep.ts <path> --max 50
 *
 * If <path> is omitted, the script tries ~/semgrep-rules, ~/src/semgrep-rules,
 * ~/Code/semgrep-rules in order. If none exists, instructions are printed.
 */

import { existsSync } from "fs";
import { join } from "path";
import { homedir } from "os";

import { importSemgrepCorpus } from "../src/fix/corpus/import-semgrep.js";

function main(): void {
  const args = process.argv.slice(2);

  const maxIdx = args.indexOf("--max");
  const max = maxIdx !== -1 && maxIdx + 1 < args.length ? parseInt(args[maxIdx + 1]!, 10) : undefined;
  const positional = args.filter((a, i) => !a.startsWith("--") && (i === 0 || !args[i - 1]?.startsWith("--")));

  let ruleDir: string | undefined = positional[0];
  if (!ruleDir) {
    const candidates = [
      join(homedir(), "semgrep-rules"),
      join(homedir(), "src", "semgrep-rules"),
      join(homedir(), "Code", "semgrep-rules"),
    ];
    ruleDir = candidates.find((c) => existsSync(c));
  }

  if (!ruleDir || !existsSync(ruleDir)) {
    process.stderr.write(
      "No semgrep-rules clone found.\n" +
        "\n" +
        "Clone it with:\n" +
        "  git clone --depth 1 https://github.com/semgrep/semgrep-rules ~/semgrep-rules\n" +
        "\n" +
        "Then re-run:\n" +
        "  npx tsx scripts/import-semgrep.ts ~/semgrep-rules\n",
    );
    process.exit(1);
  }

  const outDir = join(process.cwd(), "src", "fix", "corpus", "scenarios", "imported");
  process.stdout.write(`Importing semgrep-rules from: ${ruleDir}\n`);
  process.stdout.write(`Writing scenarios to: ${outDir}\n\n`);

  const summary = importSemgrepCorpus({ ruleDir, outDir, maxScenarios: max });

  process.stdout.write(`\nImport complete:\n`);
  process.stdout.write(`  written: ${summary.written}\n`);
  process.stdout.write(`  skipped: ${summary.skipped}\n`);
  if (Object.keys(summary.reasons).length > 0) {
    process.stdout.write(`  reasons:\n`);
    for (const [reason, count] of Object.entries(summary.reasons).sort((a, b) => b[1] - a[1])) {
      process.stdout.write(`    [${count}] ${reason}\n`);
    }
  }
}

main();
