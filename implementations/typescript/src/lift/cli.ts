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
  rpc: boolean;
  liftOptions: LiftOptions;
}

export function parseCliArgs(argv: string[]): CliFlags {
  let workspace = ".";
  let outDir: string | null = null;
  let quiet = false;
  let rpc = false;

  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--help" || a === "-h") {
      printHelp();
      process.exit(0);
    } else if (a === "--quiet" || a === "-q") {
      quiet = true;
    } else if (a === "--rpc") {
      rpc = true;
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
    rpc,
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
 * RPC mode for provekit plugin protocol.
 * Speaks JSON-RPC over stdio when --rpc is passed.
 */
function runRpcMode(): void {
  const readline = require("readline");
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  
  rl.on("line", (line: string) => {
    try {
      const req = JSON.parse(line);
      const id = req.id;
      const method = req.method;
      
      if (method === "initialize") {
        const resp = { jsonrpc: "2.0", id, result: { name: "provekit-lift-ts", version: "1.0", capabilities: [] } };
        process.stdout.write(JSON.stringify(resp) + "\n");
      } else if (method === "lift") {
        // Run the actual lift
        const { report, minted, outPath } = liftAndMint(process.cwd(), process.cwd(), defaultLiftOptions());
        const fs = require("fs");
        const bytes = fs.readFileSync(outPath);
        const b64 = bytes.toString("base64");
        const cid = minted.cid;
        const resp = { jsonrpc: "2.0", id, result: { kind: "proof-envelope", filename_cid: cid, bytes_base64: b64 } };
        process.stdout.write(JSON.stringify(resp) + "\n");
      } else if (method === "shutdown") {
        const resp = { jsonrpc: "2.0", id, result: null };
        process.stdout.write(JSON.stringify(resp) + "\n");
        rl.close();
        process.exit(0);
      }
    } catch (e) {
      const err = { jsonrpc: "2.0", id: null, error: { code: -32600, message: String(e) } };
      process.stdout.write(JSON.stringify(err) + "\n");
    }
  });
}

/**
 * Top-level entry — invoked by the `provekit-lift` bin shim.
 *
 * Exported separately from runCli so tests can inject argv without
 * triggering process.exit.
 */
export function main(argv: string[] = process.argv.slice(2)): void {
  const flags = parseCliArgs(argv);
  if (flags.rpc) {
    runRpcMode();
  } else {
    const code = runCli(flags);
    process.exit(code);
  }
}
