/**
 * `provekit mint` — mint a signed memento from a JSON specification.
 *
 * The framework's core primitive operation, exposed as a CLI command.
 * Reads a memento spec from a file (or stdin), produces a signed
 * ClaimEnvelope, writes the result to stdout (or to a file).
 *
 * Subcommands:
 *   provekit mint property [--spec <path>] [--key <path>] [--out <path>]
 *     Mints a property memento with a legacy-witness or other variant.
 *
 *   provekit mint bridge --source-symbol <s> --source-layer <l> \
 *                        --target-cid <cid> --target-layer <l> \
 *                        [--key <path>] [--out <path>]
 *     Mints a bridge memento connecting a host-language symbol to a
 *     deeper-layer published contract.
 *
 *   provekit mint catalog <dir> [--key <path>] [--out <path>]
 *     Composes all *.json memento files in <dir> into a catalog root
 *     memento whose CID becomes the kit's proofHash.
 *
 *   provekit mint --spec <path> [--key <path>] [--out <path>]
 *     Generic mint: read a full MintArgs JSON spec, sign, output.
 *
 * Key resolution:
 *   --key <path>   ed25519 PEM-encoded private key file
 *   $PROVEKIT_KEY  PEM-encoded private key (env var)
 *   default        ephemeral keypair generated, public key emitted
 *                  alongside (warning emitted to stderr)
 *
 * Scope discipline (per docs/specs/2026-04-29-correctness-is-a-hash.md
 * §"What ProvekIt is"): this command MINTS. It does NOT walk DAGs or
 * audit deeper layers.
 */

import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";
import { createPrivateKey, KeyObject, randomBytes } from "node:crypto";
import { generateKeypair } from "./producerKeys/index.js";
import {
  mintMemento,
  mintBridge,
  mintLegacyWitness,
  VARIANT_SCHEMA_CIDS,
} from "./claimEnvelope/index.js";
import type { ClaimEnvelope, EvidenceVariant } from "./claimEnvelope/types.js";
import { createHash } from "node:crypto";

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function readStdin(): string {
  // Synchronous stdin read for CLI use.
  const chunks: Buffer[] = [];
  let buf = Buffer.alloc(8192);
  let bytesRead: number;
  try {
    while (true) {
      bytesRead = require("fs").readSync(0, buf, 0, buf.length, null);
      if (bytesRead === 0) break;
      chunks.push(Buffer.from(buf.subarray(0, bytesRead)));
    }
  } catch (err: any) {
    if (err.code !== "EAGAIN") throw err;
  }
  return Buffer.concat(chunks).toString("utf8");
}

function loadPrivateKey(keyPath?: string): {
  privateKey: KeyObject;
  publicKey: KeyObject;
  ephemeral: boolean;
} {
  if (keyPath) {
    const pem = readFileSync(resolve(keyPath), "utf8");
    const privateKey = createPrivateKey({ key: pem, format: "pem" });
    const publicKey = require("node:crypto").createPublicKey(privateKey);
    return { privateKey, publicKey, ephemeral: false };
  }
  if (process.env.PROVEKIT_KEY) {
    const privateKey = createPrivateKey({
      key: process.env.PROVEKIT_KEY,
      format: "pem",
    });
    const publicKey = require("node:crypto").createPublicKey(privateKey);
    return { privateKey, publicKey, ephemeral: false };
  }
  process.stderr.write(
    "warning: no key supplied (--key or $PROVEKIT_KEY); generating ephemeral keypair.\n",
  );
  const seed = randomBytes(32);
  const kp = generateKeypair({ seed });
  return { ...kp, ephemeral: true };
}

function emitPublicKeyIfEphemeral(
  publicKey: KeyObject,
  ephemeral: boolean,
): void {
  if (!ephemeral) return;
  const spki = publicKey.export({ type: "spki", format: "der" }).toString("base64");
  process.stderr.write(`ephemeral public key (SPKI base64): ${spki}\n`);
}

function writeOutput(memento: ClaimEnvelope, outPath?: string): void {
  const json = JSON.stringify(memento, null, 2);
  if (outPath) {
    writeFileSync(outPath, json + "\n");
    process.stderr.write(`wrote memento → ${outPath}\n`);
  } else {
    process.stdout.write(json + "\n");
  }
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

interface PropertySpec {
  bindingHash: string;
  propertyHash: string;
  verdict?: "holds" | "violated" | "decayed" | "undecidable" | "error";
  producedBy: string;
  producedAt?: string;
  inputCids?: string[];
  evidence?: EvidenceVariant;
  rawWitness?: string;
}

function mintPropertyCmd(args: {
  specPath?: string;
  keyPath?: string;
  outPath?: string;
}): ClaimEnvelope {
  const specJson = args.specPath
    ? readFileSync(resolve(args.specPath), "utf8")
    : readStdin();
  const spec: PropertySpec = JSON.parse(specJson);

  const { privateKey, publicKey, ephemeral } = loadPrivateKey(args.keyPath);

  const evidence: EvidenceVariant = spec.evidence ?? {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
    body: {
      rawWitness: spec.rawWitness ?? "{}",
      legacyProducerId: spec.producedBy,
    },
  };

  const memento = mintMemento({
    bindingHash: spec.bindingHash,
    propertyHash: spec.propertyHash,
    verdict: spec.verdict ?? "holds",
    producedBy: spec.producedBy,
    ...(spec.producedAt !== undefined ? { producedAt: spec.producedAt } : {}),
    inputCids: spec.inputCids ?? [],
    evidence,
    privateKey,
  });

  emitPublicKeyIfEphemeral(publicKey, ephemeral);
  writeOutput(memento, args.outPath);
  return memento;
}

interface BridgeArgs {
  sourceSymbol: string;
  sourceLayer: string;
  targetContractCid: string;
  targetLayer: string;
  notes?: string;
  producedBy?: string;
  bindingHash?: string;
  propertyHash?: string;
  keyPath?: string;
  outPath?: string;
}

function mintBridgeCmd(args: BridgeArgs): ClaimEnvelope {
  const { privateKey, publicKey, ephemeral } = loadPrivateKey(args.keyPath);

  const producedBy = args.producedBy ?? `${args.sourceLayer}@cli`;
  const bindingHash = args.bindingHash ?? hash16(`${args.sourceLayer}:${args.sourceSymbol}`);
  const propertyHash = args.propertyHash ?? hash16(`bridge:${args.sourceSymbol}`);

  const memento = mintBridge({
    bindingHash,
    propertyHash,
    producedBy,
    privateKey,
    sourceSymbol: args.sourceSymbol,
    sourceLayer: args.sourceLayer,
    targetContractCid: args.targetContractCid,
    targetLayer: args.targetLayer,
    ...(args.notes !== undefined ? { notes: args.notes } : {}),
  });

  emitPublicKeyIfEphemeral(publicKey, ephemeral);
  writeOutput(memento, args.outPath);
  return memento;
}

function mintCatalogCmd(args: {
  dir: string;
  producedBy?: string;
  catalogName?: string;
  catalogVersion?: string;
  keyPath?: string;
  outPath?: string;
}): ClaimEnvelope {
  const dir = resolve(args.dir);
  const files = readdirSync(dir)
    .filter((f) => f.endsWith(".json") && f !== "catalog.json")
    .sort();

  const memementoFiles = files.filter((f) => {
    try {
      const obj = JSON.parse(readFileSync(join(dir, f), "utf8"));
      return typeof obj.cid === "string" && typeof obj.bindingHash === "string";
    } catch {
      return false;
    }
  });

  const cids: string[] = [];
  for (const f of memementoFiles) {
    const memento: ClaimEnvelope = JSON.parse(readFileSync(join(dir, f), "utf8"));
    cids.push(memento.cid);
  }
  cids.sort();

  const { privateKey, publicKey, ephemeral } = loadPrivateKey(args.keyPath);

  const catalogName = args.catalogName ?? "catalog";
  const catalogVersion = args.catalogVersion ?? "0.0.1";
  const producedBy = args.producedBy ?? `${catalogName}@${catalogVersion}`;

  const root = mintLegacyWitness({
    bindingHash: hash16(`${catalogName}@${catalogVersion}`),
    propertyHash: hash16(`catalog-root:${catalogName}@${catalogVersion}`),
    verdict: "holds",
    producedBy,
    inputCids: cids,
    privateKey,
    rawWitness: JSON.stringify({
      kind: "catalog",
      name: catalogName,
      version: catalogVersion,
      memberCount: cids.length,
      members: cids,
    }),
  });

  emitPublicKeyIfEphemeral(publicKey, ephemeral);
  writeOutput(root, args.outPath);
  process.stderr.write(`composed ${cids.length} mementos into catalog root\n`);
  return root;
}

function genericMintCmd(args: {
  specPath?: string;
  keyPath?: string;
  outPath?: string;
}): ClaimEnvelope {
  const specJson = args.specPath
    ? readFileSync(resolve(args.specPath), "utf8")
    : readStdin();
  const spec = JSON.parse(specJson) as Omit<PropertySpec, "rawWitness"> & {
    evidence: EvidenceVariant;
  };

  const { privateKey, publicKey, ephemeral } = loadPrivateKey(args.keyPath);

  const memento = mintMemento({
    bindingHash: spec.bindingHash,
    propertyHash: spec.propertyHash,
    verdict: spec.verdict ?? "holds",
    producedBy: spec.producedBy,
    ...(spec.producedAt !== undefined ? { producedAt: spec.producedAt } : {}),
    inputCids: spec.inputCids ?? [],
    evidence: spec.evidence,
    privateKey,
  });

  emitPublicKeyIfEphemeral(publicKey, ephemeral);
  writeOutput(memento, args.outPath);
  return memento;
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

function parseFlags(argv: string[]): Record<string, string | true> {
  const flags: Record<string, string | true> = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (!a.startsWith("--")) continue;
    const name = a.slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith("--")) {
      flags[name] = true;
    } else {
      flags[name] = next;
      i++;
    }
  }
  return flags;
}

function printMintHelp(): void {
  process.stderr.write(`provekit mint — mint a signed memento

Usage:
  provekit mint property  [--spec <path>] [--key <path>] [--out <path>]
  provekit mint bridge --source-symbol <s> --source-layer <l>
                       --target-cid <cid> --target-layer <l>
                       [--bindingHash <h>] [--propertyHash <h>]
                       [--produced-by <id>] [--notes <text>]
                       [--key <path>] [--out <path>]
  provekit mint catalog <dir> [--name <s>] [--version <s>]
                              [--produced-by <id>]
                              [--key <path>] [--out <path>]
  provekit mint generic   [--spec <path>] [--key <path>] [--out <path>]

Property and generic read JSON spec from --spec or stdin.

Key resolution: --key <path> > $PROVEKIT_KEY > ephemeral keypair (with
public-key emitted on stderr).

Output: --out <path> writes file; otherwise JSON written to stdout.
Stderr is reserved for warnings and progress.
`);
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

export async function runMint(argv: string[]): Promise<void> {
  if (argv.length === 0 || argv[0] === "--help" || argv[0] === "-h") {
    printMintHelp();
    return;
  }

  const subcommand = argv[0]!;
  const rest = argv.slice(1);
  const flags = parseFlags(rest);

  switch (subcommand) {
    case "property": {
      mintPropertyCmd({
        specPath: typeof flags.spec === "string" ? flags.spec : undefined,
        keyPath: typeof flags.key === "string" ? flags.key : undefined,
        outPath: typeof flags.out === "string" ? flags.out : undefined,
      });
      break;
    }
    case "bridge": {
      const required = ["source-symbol", "source-layer", "target-cid", "target-layer"];
      for (const r of required) {
        if (typeof flags[r] !== "string") {
          process.stderr.write(`error: --${r} required for 'mint bridge'\n`);
          process.exit(1);
        }
      }
      mintBridgeCmd({
        sourceSymbol: flags["source-symbol"] as string,
        sourceLayer: flags["source-layer"] as string,
        targetContractCid: flags["target-cid"] as string,
        targetLayer: flags["target-layer"] as string,
        notes: typeof flags.notes === "string" ? flags.notes : undefined,
        producedBy:
          typeof flags["produced-by"] === "string"
            ? (flags["produced-by"] as string)
            : undefined,
        bindingHash:
          typeof flags.bindingHash === "string" ? (flags.bindingHash as string) : undefined,
        propertyHash:
          typeof flags.propertyHash === "string" ? (flags.propertyHash as string) : undefined,
        keyPath: typeof flags.key === "string" ? flags.key : undefined,
        outPath: typeof flags.out === "string" ? flags.out : undefined,
      });
      break;
    }
    case "catalog": {
      const dir = rest.find((a) => !a.startsWith("--"));
      if (!dir) {
        process.stderr.write(`error: 'mint catalog' requires <dir>\n`);
        process.exit(1);
      }
      mintCatalogCmd({
        dir,
        catalogName: typeof flags.name === "string" ? flags.name : undefined,
        catalogVersion: typeof flags.version === "string" ? flags.version : undefined,
        producedBy:
          typeof flags["produced-by"] === "string"
            ? (flags["produced-by"] as string)
            : undefined,
        keyPath: typeof flags.key === "string" ? flags.key : undefined,
        outPath: typeof flags.out === "string" ? flags.out : undefined,
      });
      break;
    }
    case "generic": {
      genericMintCmd({
        specPath: typeof flags.spec === "string" ? flags.spec : undefined,
        keyPath: typeof flags.key === "string" ? flags.key : undefined,
        outPath: typeof flags.out === "string" ? flags.out : undefined,
      });
      break;
    }
    default: {
      process.stderr.write(`unknown 'mint' subcommand: ${subcommand}\n`);
      printMintHelp();
      process.exit(1);
    }
  }
}
