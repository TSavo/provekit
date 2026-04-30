/**
 * `provekit attest` — walk the project's *.invariant.ts files, run each
 * inside a collector, mint mementos for every declaration, compose a
 * project root, identify null roots.
 *
 * The ATTEST primitive: produce signed mementos for every invariant
 * declaration the framework can find in the project, and report the
 * exact list of code paths that are NOT verified correct (null roots).
 *
 * Exit codes:
 *   0   — zero null roots; the project is provably correct under the
 *         locally-available kit catalogs
 *   1   — one or more null roots; the project's correctness is
 *         incomplete (specific code paths reported)
 *
 * Scope discipline (per docs/specs/2026-04-29-correctness-is-a-hash.md
 * §"What ProvekIt is"): this command MINTS local mementos and IDENTIFIES
 * null roots. It does NOT walk into external bridge targets (audit work,
 * downstream tooling). It does NOT invoke Z3 (proof-stage tooling, not
 * yet wired). The hash chain is reported; SAT solving is a separate
 * primitive.
 */

import { readdirSync, statSync, readFileSync, existsSync, writeFileSync, mkdirSync } from "node:fs";
import { join, resolve, relative } from "node:path";
import { createHash, randomBytes } from "node:crypto";
import {
  generateKeypair,
} from "./producerKeys/index.js";
import {
  runVerifyProjectInvariants,
  type InvariantFileSource,
  type VerifyProjectInvariantsStageInput,
} from "./workflow/producers/verifyProjectInvariants.js";

const HELP = `provekit attest — produce signed mementos for project invariants

Usage:
  provekit attest [project-root]
                  [--key <path>]      ed25519 PEM private key
                  [--out <dir>]       write per-declaration mementos
                  [--ci]              exit 1 if any null root

Walks <project-root>/src for *.invariant.ts files, runs each inside the
provekit collector, mints mementos for every declaration, composes a
project root, identifies null roots.

Exit 0 — zero null roots; the project is provably correct against the
        locally-available kit catalogs.
Exit 1 — one or more null roots; the project's correctness is
        incomplete; specific gaps named.
`;

function sha256Hex(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}

function findInvariantFiles(root: string): InvariantFileSource[] {
  const found: InvariantFileSource[] = [];
  if (!existsSync(root)) return found;

  function walk(dir: string): void {
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      return;
    }
    for (const entry of entries) {
      const full = join(dir, entry);
      let s;
      try {
        s = statSync(full);
      } catch {
        continue;
      }
      if (s.isDirectory()) {
        if (entry === "node_modules" || entry === "dist" || entry === "lib" || entry === "__fixtures__" || entry.startsWith(".")) continue;
        walk(full);
      } else if (entry.endsWith(".invariant.ts") || entry.endsWith(".invariant.mjs") || entry.endsWith(".invariant.js")) {
        const content = readFileSync(full, "utf8");
        found.push({
          path: relative(root, full),
          contentHash: sha256Hex(content),
          resolvedModulePath: full,
        });
      }
    }
  }

  walk(root);
  return found;
}

function parseFlags(argv: string[]): {
  positional: string[];
  flags: Record<string, string | true>;
} {
  const positional: string[] = [];
  const flags: Record<string, string | true> = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (!a.startsWith("--")) {
      positional.push(a);
      continue;
    }
    const name = a.slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith("--")) {
      flags[name] = true;
    } else {
      flags[name] = next;
      i++;
    }
  }
  return { positional, flags };
}

function loadOrGenerateKeypair(keyPath: string | undefined) {
  if (keyPath) {
    const { createPrivateKey, createPublicKey } = require("node:crypto");
    const pem = readFileSync(resolve(keyPath), "utf8");
    const privateKey = createPrivateKey({ key: pem, format: "pem" });
    const publicKey = createPublicKey(privateKey);
    return { privateKey, publicKey, ephemeral: false };
  }
  if (process.env.PROVEKIT_KEY) {
    const { createPrivateKey, createPublicKey } = require("node:crypto");
    const privateKey = createPrivateKey({ key: process.env.PROVEKIT_KEY, format: "pem" });
    const publicKey = createPublicKey(privateKey);
    return { privateKey, publicKey, ephemeral: false };
  }
  process.stderr.write(
    "warning: no key supplied (--key or $PROVEKIT_KEY); generating ephemeral keypair.\n",
  );
  const seed = randomBytes(32);
  const kp = generateKeypair({ seed });
  return { ...kp, ephemeral: true };
}

export async function runAttest(argv: string[]): Promise<void> {
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(HELP);
    return;
  }

  const { positional, flags } = parseFlags(argv);
  const root = resolve(positional[0] ?? process.cwd());
  const srcDir = join(root, "src");
  const ciMode = flags.ci === true;
  const keyPath = typeof flags.key === "string" ? flags.key : undefined;
  const outDir = typeof flags.out === "string" ? resolve(flags.out) : null;

  // Find all invariant files
  const invariantFiles = findInvariantFiles(srcDir);
  if (invariantFiles.length === 0) {
    process.stderr.write(
      `no *.invariant.ts files found under ${srcDir}\n`,
    );
    process.exit(0);
  }

  const projectName = readPackageName(root);
  const projectVersion = readPackageVersion(root);

  process.stderr.write(`provekit attest ${projectName}@${projectVersion}\n`);
  process.stderr.write(`  scanning ${srcDir}\n`);
  process.stderr.write(`  found ${invariantFiles.length} invariant file(s)\n`);

  const { privateKey, ephemeral } = loadOrGenerateKeypair(keyPath);

  const input: VerifyProjectInvariantsStageInput = {
    projectName,
    projectVersion,
    invariantFiles,
    locallyAvailableCids: [], // v1: no node_modules walk; treat all bridge targets as null roots
  };

  const out = await runVerifyProjectInvariants(input, {
    privateKey,
    producerId: `attest@${projectName}`,
    producedAt: new Date().toISOString(),
  });

  // Report
  process.stderr.write(`\n${out.declarations.length} declarations minted:\n`);
  for (const d of out.declarations) {
    process.stderr.write(
      `  ${d.declarationKind.padEnd(8)} ${d.declarationName.padEnd(60)} cid: ${d.cid}\n`,
    );
  }

  process.stderr.write(`\nproject root cid: ${out.projectRootCid}\n`);

  if (out.nullRoots.length === 0) {
    process.stderr.write(
      `\n✓ provably correct: 0 null roots (every reference resolves locally)\n`,
    );
  } else {
    process.stderr.write(
      `\n✗ verification incomplete: ${out.nullRoots.length} null root(s) — these code paths are NOT verified correct\n`,
    );
    for (const cid of out.nullRoots) {
      process.stderr.write(`    ${cid}\n`);
    }
  }

  if (ephemeral) {
    process.stderr.write(
      `\nnote: ephemeral keypair used; install a producer key for persistent attestation\n`,
    );
  }

  // Optional: write mementos to disk
  if (outDir) {
    if (!existsSync(outDir)) mkdirSync(outDir, { recursive: true });
    const summary = {
      projectName,
      projectVersion,
      projectRootCid: out.projectRootCid,
      declarations: out.declarations,
      nullRoots: out.nullRoots,
    };
    writeFileSync(join(outDir, "attest-summary.json"), JSON.stringify(summary, null, 2));
    process.stderr.write(`\nsummary written to ${join(outDir, "attest-summary.json")}\n`);
  }

  // Exit code
  if (ciMode && out.nullRoots.length > 0) {
    process.exit(1);
  }
}

function readPackageName(root: string): string {
  const pkgPath = join(root, "package.json");
  if (!existsSync(pkgPath)) return "unknown-project";
  try {
    return JSON.parse(readFileSync(pkgPath, "utf8")).name ?? "unknown-project";
  } catch {
    return "unknown-project";
  }
}

function readPackageVersion(root: string): string {
  const pkgPath = join(root, "package.json");
  if (!existsSync(pkgPath)) return "0.0.0";
  try {
    return JSON.parse(readFileSync(pkgPath, "utf8")).version ?? "0.0.0";
  } catch {
    return "0.0.0";
  }
}
