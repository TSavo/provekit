#!/usr/bin/env node
/**
 * provekit / proveit — protocol-first CLI.
 *
 * Surface (everything else has been deleted as not-in-service-of-the-protocol):
 *   provekit verify           — bridge enforcement: walk .proof files,
 *                                discharge per-callsite IR obligations
 *                                via the configured solver
 *   provekit mint <subcmd>    — mint signed mementos (property /
 *                                bridge / catalog / generic)
 *   provekit dump <file>.proof — inspect a .proof bundle
 *   provekit override --reason — record an intentional bypass for one commit
 *   provekit --version        — print version
 *
 * Verb aliases (purely lexical):
 *   will / always / shall  → must
 *   verifies              → verify
 *   changes               → change
 *   proves                → prove
 */

import { resolve, join } from "path";
import { existsSync } from "fs";
import { runMint } from "./cli.mint.js";
import { runDump } from "./cli.dump.js";

const VERSION = "0.4.0";

const COMMAND_ALIASES: Readonly<Record<string, string>> = Object.freeze({
  will: "must",
  always: "must",
  shall: "must",
  verifies: "verify",
  changes: "change",
  proves: "prove",
});

export function expandCommandAlias(argv: string[]): string[] {
  if (argv.length === 0) return argv;
  const first = argv[0]!;
  const canonical = COMMAND_ALIASES[first];
  if (!canonical) return argv;
  return [canonical, ...argv.slice(1)];
}

async function main(): Promise<void> {
  const args = expandCommandAlias(process.argv.slice(2));

  if (args.length === 0 || args.includes("--help") || args.includes("-h")) {
    printHelp();
    process.exit(args.length === 0 ? 1 : 0);
  }

  if (args[0] === "--version") {
    console.log(`provekit v${VERSION}`);
    console.log("The Kit to Prove It's Fixed.");
    process.exit(0);
  }

  const command = args[0]!;
  const rest = args.slice(1);

  switch (command) {
    case "verify":   await runVerify(rest); break;
    case "mint":     await runMint(rest); break;
    case "dump":     await runDump(rest); break;
    case "override": runOverride(rest); break;
    default:
      console.error(`Unknown command: ${command}`);
      printHelp();
      process.exit(1);
  }
}

function printHelp(): void {
  console.error(`provekit v${VERSION} — protocol-first verifier

Usage:
  provekit verify [--ci]              Discharge bridge call-site obligations
  provekit mint <subcmd> [...]        Mint a signed memento (property/bridge/catalog/generic)
  provekit dump <file>.proof [--json] Inspect a .proof bundle
  provekit override --reason "..."    Record intentional bypass for one commit
  provekit --version                  Print version

Aliases:
  proveit ↔ provekit
  will / always / shall → must
  verifies → verify
  changes → change
  proves → prove
`);
}

function resolveProjectRoot(args: string[]): string {
  const flagIdx = args.indexOf("--project");
  if (flagIdx >= 0 && args[flagIdx + 1]) {
    return resolve(args[flagIdx + 1]!);
  }
  let dir = resolve(".");
  while (dir !== "/" && dir !== "") {
    if (
      existsSync(join(dir, ".provekit")) ||
      existsSync(join(dir, "package.json")) ||
      existsSync(join(dir, "Cargo.toml"))
    ) {
      return dir;
    }
    const parent = resolve(dir, "..");
    if (parent === dir) break;
    dir = parent;
  }
  return resolve(".");
}

async function runVerify(args: string[]): Promise<void> {
  const projectRoot = resolveProjectRoot(args);
  const ci = args.includes("--ci");

  console.log(`provekit v${VERSION} — verify (bridge enforcement, IR-substrate)`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  const { runBridgeEnforcement, formatBridgeEnforcementReport } = await import(
    "./verifier/bridgeEnforcement.js"
  );
  const bridgeReport = await runBridgeEnforcement(projectRoot);
  console.log("Bridge enforcement:");
  process.stdout.write(formatBridgeEnforcementReport(bridgeReport));

  if (ci && bridgeReport.violations > 0) {
    console.log();
    console.log(
      `${bridgeReport.violations} bridge violation${bridgeReport.violations === 1 ? "" : "s"} found.`,
    );
    process.exit(1);
  }
  process.exit(0);
}

function runOverride(args: string[]): void {
  const reasonIdx = args.indexOf("--reason");
  const reason = reasonIdx >= 0 ? args[reasonIdx + 1] : undefined;
  if (!reason) {
    console.error('Usage: provekit override --reason "why this is intentional"');
    process.exit(1);
  }
  console.log(`Override recorded: ${reason}`);
  console.log("Run: git commit --no-verify");
}

// Only auto-run when invoked as the entry point (not when imported by
// tests). The bin/provekit.cjs launcher imports this file via tsx,
// which sets argv[1] to this script's path; tests import without that.
if (require.main === module) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
