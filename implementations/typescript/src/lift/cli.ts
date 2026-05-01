/**
 * `provekit-lift` — CLI entry point for the TS lift toolchain.
 *
 * Usage:
 *   provekit-lift [<dir>] [--out <dir>] [--quiet]
 *
 * Walks <dir> (default: cwd), lifts every recognized zod schema and
 * fast-check property, mints a `.proof` catalog, prints the CID. Zero
 * source changes required to consume.
 */

import { resolve } from "node:path";
import { liftAndMint, defaultLiftOptions, type LiftOptions } from "./index.js";

export interface CliFlags {
  workspace: string;
  outDir: string | null;
  quiet: boolean;
  liftOptions: LiftOptions;
}

export function parseCliArgs(argv: string[]): CliFlags {
  let workspace = ".";
  let outDir: string | null = null;
  let quiet = false;

  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--help" || a === "-h") {
      printHelp();
      process.exit(0);
    } else if (a === "--quiet" || a === "-q") {
      quiet = true;
    } else if (a === "--out" || a === "-o" || a === "--target-dir") {
      const next = argv[++i];
      if (next === undefined) {
        process.stderr.write(`provekit-lift: missing argument for ${a}\n`);
        process.exit(2);
      }
      outDir = next;
    } else if (a === "--workspace" || a === "-w") {
      const next = argv[++i];
      if (next === undefined) {
        process.stderr.write(`provekit-lift: missing argument for ${a}\n`);
        process.exit(2);
      }
      workspace = next;
    } else if (a.startsWith("-")) {
      process.stderr.write(`provekit-lift: unrecognized flag: ${a}\n`);
      process.exit(2);
    } else {
      // First positional becomes the workspace argument.
      workspace = a;
    }
  }

  return {
    workspace,
    outDir,
    quiet,
    liftOptions: defaultLiftOptions(),
  };
}

function printHelp(): void {
  process.stdout.write(
    `provekit-lift — promote existing zod schemas and fast-check properties to signed contracts.

USAGE:
  provekit-lift [<workspace-dir>] [--out <dir>] [--quiet]

FLAGS:
  --out <dir>    Output directory for the .proof file. Default: <workspace>.
  --quiet        Suppress per-adapter summary; print only the CID.
  --help         Show this help.

POSITIONING:
  ProvekIt does NOT compete with zod, fast-check, io-ts, yup, joi,
  class-validator, or valibot. It sits BENEATH them. We promote what
  you already have to content-addressed signed contracts. The
  proveLift LLM pipeline is the fallback for greenfield code.
`,
  );
}

export function runCli(flags: CliFlags): number {
  const workspace = resolve(flags.workspace);
  const outDir = resolve(flags.outDir ?? flags.workspace);

  try {
    const { report, minted, outPath } = liftAndMint(workspace, outDir, flags.liftOptions);
    if (flags.quiet) {
      process.stdout.write(`${minted.cid}\n`);
    } else {
      process.stdout.write(
        `provekit-lift: scanned ${report.filesScanned} TypeScript files\n`,
      );
      for (const ar of report.adapterReports) {
        process.stdout.write(
          `  adapter "${ar.adapter}": seen ${ar.seen}, lifted ${ar.lifted}, skipped ${ar.warnings.length}\n`,
        );
        for (const w of ar.warnings) {
          process.stderr.write(
            `    warn: ${w.adapter} skipped "${w.itemName}" in ${w.sourcePath}: ${w.reason}\n`,
          );
        }
      }
      if (minted.deduplicated > 0) {
        process.stdout.write(
          `  dedup: ${minted.deduplicated} contracts collapsed by content address\n`,
        );
      }
      process.stdout.write(
        `provekit-lift: wrote ${outPath} (${minted.memberCount} members)\n`,
      );
      process.stdout.write(`provekit-lift: cid = ${minted.cid}\n`);
    }
    return 0;
  } catch (e) {
    process.stderr.write(`provekit-lift: ${(e as Error).message}\n`);
    return 1;
  }
}

/**
 * Top-level entry — invoked by the `provekit-lift` bin shim.
 *
 * Exported separately from runCli so tests can inject argv without
 * triggering process.exit.
 */
export function main(argv: string[] = process.argv.slice(2)): void {
  const flags = parseCliArgs(argv);
  const code = runCli(flags);
  process.exit(code);
}
