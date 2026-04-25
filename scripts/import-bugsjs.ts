#!/usr/bin/env tsx
/**
 * scripts/import-bugsjs.ts — generate corpus scenarios from a local clone of
 * github.com/BugsJS/bugs-data.
 *
 * Usage:
 *   npx tsx scripts/import-bugsjs.ts <path-to-bugs-data>
 *   npx tsx scripts/import-bugsjs.ts <path> --max 30
 *   npx tsx scripts/import-bugsjs.ts <path> --max 30 --max-files 2 --max-loc 50
 *
 * If <path> is omitted, the script tries ~/bugsjs-data, ~/src/bugsjs-data,
 * ~/Code/bugsjs-data in order. If none exists, instructions are printed.
 *
 * The user is expected to clone the descriptors repo themselves:
 *   git clone --depth=1 https://github.com/BugsJS/bugs-data.git ~/bugsjs-data
 *
 * Output: per-bug scenario files under
 *   src/fix/corpus/scenarios/imported/bugsjs/<project>-<bugId>.ts
 */

import { existsSync } from "fs";
import { join } from "path";
import { homedir } from "os";

import { importBugsJsCorpus } from "../src/fix/corpus/import-bugsjs.js";

function parseFlag(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return undefined;
  if (idx + 1 >= args.length) return undefined;
  return args[idx + 1];
}

function main(): void {
  const args = process.argv.slice(2);

  const maxStr = parseFlag(args, "--max");
  const max = maxStr !== undefined ? parseInt(maxStr, 10) : undefined;

  const maxFilesStr = parseFlag(args, "--max-files");
  const maxFiles = maxFilesStr !== undefined ? parseInt(maxFilesStr, 10) : undefined;

  const maxLocStr = parseFlag(args, "--max-loc");
  const maxLoc = maxLocStr !== undefined ? parseInt(maxLocStr, 10) : undefined;

  // First positional arg that does not follow a flag.
  const positional: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i]!;
    if (a.startsWith("--")) {
      // Skip the flag and its value.
      i += 1;
      continue;
    }
    positional.push(a);
  }

  let dataDir: string | undefined = positional[0];
  if (!dataDir) {
    const candidates = [
      join(homedir(), "bugsjs-data"),
      join(homedir(), "src", "bugsjs-data"),
      join(homedir(), "Code", "bugsjs-data"),
    ];
    dataDir = candidates.find((c) => existsSync(c));
  }

  if (!dataDir || !existsSync(dataDir)) {
    process.stderr.write(
      "No bugsjs-data clone found.\n" +
        "\n" +
        "Clone it with:\n" +
        "  git clone --depth=1 https://github.com/BugsJS/bugs-data.git ~/bugsjs-data\n" +
        "\n" +
        "Then re-run:\n" +
        "  npx tsx scripts/import-bugsjs.ts ~/bugsjs-data\n",
    );
    process.exit(1);
  }

  const outDir = join(
    process.cwd(),
    "src",
    "fix",
    "corpus",
    "scenarios",
    "imported",
    "bugsjs",
  );
  process.stdout.write(`Importing bugs-data from: ${dataDir}\n`);
  process.stdout.write(`Writing scenarios to: ${outDir}\n\n`);

  const summary = importBugsJsCorpus({
    dataDir,
    outDir,
    maxBugs: max,
    maxFilesPerBug: maxFiles,
    maxLocPerFile: maxLoc,
  });

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
