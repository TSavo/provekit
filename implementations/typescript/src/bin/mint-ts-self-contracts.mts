#!/usr/bin/env -S npx tsx
// SPDX-License-Identifier: Apache-2.0
//
// mint-ts-self-contracts — TypeScript peer-implementation orchestrator.
//
// 1. Walks every .invariant.ts file in implementations/typescript/src/
//    (one per public-API source file).
// 2. For each slab: beginCollecting() -> import slab -> finish().
// 3. Mints each contract as a signed memento under the foundation key
//    (test seed [0x42; 32]) and bundles into a single .proof envelope
//    whose filename IS its CID.
// 4. Mints twice into separate output directories and asserts byte-
//    deterministic CIDs.
//
// The repo's other tsx-driven binaries (`bin/provekit.cjs`,
// `bin/provekit-lift.cjs`) are currently broken on Node 25 because
// @ipld/dag-cbor is ESM-only and tsx's CJS bridge can't resolve it.
// Vitest's Vite loader handles ESM cleanly — that's the working
// invocation:
//
//   pnpm vitest run implementations/typescript/src/bin/mint-ts-self-contracts.test.ts
//
// That test exercises `runMintSelfContracts` below and prints the
// catalog CID + determinism status.

import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";

import { mintContract } from "../claimEnvelope/mint.js";
import { buildProofEnvelope } from "../proofEnvelope/index.js";
import { generateKeypair } from "../producerKeys/index.js";
import { computeCid } from "../canonicalizer/hash.js";
import {
  beginCollecting,
  _resetCollector,
  type Declaration,
  type ContractDeclaration,
} from "../ir/symbolic/property.js";

import { invariants as hashInvariants } from "../canonicalizer/hash.invariant.js";
import { invariants as jcsInvariants } from "../canonicalizer/jcs.invariant.js";
import { invariants as propertyHashInvariants } from "../canonicalizer/propertyHash.invariant.js";
import { invariants as claimEnvelopeMintInvariants } from "../claimEnvelope/mint.invariant.js";
import { invariants as claimEnvelopeSignInvariants } from "../claimEnvelope/sign.invariant.js";
import { invariants as claimEnvelopeCidInvariants } from "../claimEnvelope/cid.invariant.js";
import { invariants as proofEnvelopeInvariants } from "../proofEnvelope/index.invariant.js";
import { invariants as verifierInvariants } from "../verifier/index.invariant.js";
import { invariants as verifierBridgeInvariants } from "../verifier/bridgeEnforcement.invariant.js";
import { invariants as proofResolverInvariants } from "../proofResolver/index.invariant.js";
import { invariants as liftInvariants } from "../lift/index.invariant.js";
import { invariants as zodAdapterInvariants } from "../lift/adapters/zod.invariant.js";
import { invariants as vitestTestsAdapterInvariants } from "../lift/adapters/vitest-tests.invariant.js";
import { invariants as crossKitBridgesInvariants } from "../lift/cross-kit-bridges.invariant.js";

export const PRODUCED_BY = "@provekit/ts-self-contracts@1.0";
export const DECLARED_AT = "2026-04-30T18:00:00.000Z";

export interface InvariantSource {
  label: string;
  path: string;
  fn: () => void;
}

const SLABS: InvariantSource[] = [
  {
    label: "hash",
    path: "implementations/typescript/src/canonicalizer/hash.ts",
    fn: hashInvariants,
  },
  {
    label: "jcs",
    path: "implementations/typescript/src/canonicalizer/jcs.ts",
    fn: jcsInvariants,
  },
  {
    label: "propertyHash",
    path: "implementations/typescript/src/canonicalizer/canonicalize.ts",
    fn: propertyHashInvariants,
  },
  {
    label: "claim-mint",
    path: "implementations/typescript/src/claimEnvelope/mint.ts",
    fn: claimEnvelopeMintInvariants,
  },
  {
    label: "claim-sign",
    path: "implementations/typescript/src/claimEnvelope/sign.ts",
    fn: claimEnvelopeSignInvariants,
  },
  {
    label: "claim-cid",
    path: "implementations/typescript/src/claimEnvelope/cid.ts",
    fn: claimEnvelopeCidInvariants,
  },
  {
    label: "proof-envelope",
    path: "implementations/typescript/src/proofEnvelope/index.ts",
    fn: proofEnvelopeInvariants,
  },
  {
    label: "verifier",
    path: "implementations/typescript/src/verifier/index.ts",
    fn: verifierInvariants,
  },
  {
    label: "verifier-bridge",
    path: "implementations/typescript/src/verifier/bridgeEnforcement.ts",
    fn: verifierBridgeInvariants,
  },
  {
    label: "proof-resolver",
    path: "implementations/typescript/src/proofResolver/index.ts",
    fn: proofResolverInvariants,
  },
  {
    label: "lift",
    path: "implementations/typescript/src/lift/index.ts",
    fn: liftInvariants,
  },
  {
    label: "lift-zod",
    path: "implementations/typescript/src/lift/adapters/zod.ts",
    fn: zodAdapterInvariants,
  },
  {
    label: "lift-vitest-tests",
    path: "implementations/typescript/src/lift/adapters/vitest-tests.ts",
    fn: vitestTestsAdapterInvariants,
  },
  {
    label: "cross-kit-bridges",
    path: "implementations/typescript/src/lift/cross-kit-bridges.ts",
    fn: crossKitBridgesInvariants,
  },
];

interface AuthoredSlab {
  source: InvariantSource;
  contracts: ContractDeclaration[];
}

function authorAllInvariants(): AuthoredSlab[] {
  const slabs: AuthoredSlab[] = [];
  for (const src of SLABS) {
    _resetCollector();
    const finish = beginCollecting();
    src.fn();
    const decls: Declaration[] = finish();
    const contracts = decls.filter(
      (d): d is ContractDeclaration => d.kind === "contract",
    );
    slabs.push({ source: src, contracts });
  }
  return slabs;
}

export interface MintResult {
  cid: string;
  bytesLen: number;
  path: string;
  memberCount: number;
  totalContracts: number;
  perSourceCounts: { label: string; count: number }[];
}

/**
 * Mint all 14 .invariant.ts slabs as signed mementos, register no
 * bridges (TS dogfood's symbolic surface has no closed-loop bridge to
 * register the way Rust's parse_formula does), bundle into a `.proof`,
 * write to `<outDir>/<full-cid>.proof`, return the result.
 */
export function runMintSelfContracts(outDir: string): MintResult {
  mkdirSync(outDir, { recursive: true });

  // Reset cross-run state every mint pass: quantifier counter and any
  // active collector. Without this, run B's `_x0/_x1` quantifier-bound
  // names diverge from run A's because the counter is process-global.
  _resetCollector();

  const slabs = authorAllInvariants();

  // Foundation key — identical to Rust's [0x42; 32] test seed. Same key
  // means same signature for the same canonical-encoded payload across
  // peer impls (where the hashed bytes themselves match across impls).
  const seed = Buffer.alloc(32, 0x42);
  const { privateKey, publicKey } = generateKeypair({ seed });

  const members = new Map();
  const seenNames = new Set<string>();
  const perSourceCounts: { label: string; count: number }[] = [];
  let total = 0;

  for (const slab of slabs) {
    perSourceCounts.push({
      label: slab.source.label,
      count: slab.contracts.length,
    });
    total += slab.contracts.length;
    for (const d of slab.contracts) {
      if (seenNames.has(d.name)) {
        throw new Error(
          `duplicate contract name \`${d.name}\` across .invariant.ts files`,
        );
      }
      seenNames.add(d.name);

      const env = mintContract({
        producedBy: PRODUCED_BY,
        producedAt: DECLARED_AT,
        privateKey,
        contractName: d.name,
        outBinding: d.outBinding,
        ...(d.pre !== undefined ? { pre: d.pre } : {}),
        ...(d.post !== undefined ? { post: d.post } : {}),
        ...(d.inv !== undefined ? { inv: d.inv } : {}),
        authoring: {
          producerKind: "kit-author",
          author: PRODUCED_BY,
          note: `self-contract from ${slab.source.path}`,
        },
      });
      members.set(env.cid, env);
    }
  }

  // Signer CID: BLAKE3-512 over the SPKI-DER pubkey bytes. Matches the
  // TS kit's existing convention (lift/index.ts ships the same shape).
  const pubDer = publicKey.export({ type: "spki", format: "der" }) as Buffer;
  const signerCid = computeCid(pubDer);

  const built = buildProofEnvelope({
    name: "@provekit/ts-self-contracts",
    version: "1.0.0",
    members,
    signerCid,
    signerPrivateKey: privateKey,
    declaredAt: DECLARED_AT,
  });

  if (!built.cid.startsWith("blake3-512:")) {
    throw new Error("internal: cid missing blake3-512 prefix");
  }
  const path = join(outDir, `${built.cid}.proof`);
  writeFileSync(path, Buffer.from(built.bytes));

  return {
    cid: built.cid,
    bytesLen: built.bytes.length,
    path,
    memberCount: members.size,
    totalContracts: total,
    perSourceCounts,
  };
}

export function main(argv: string[]): number {
  const outDir = argv[2] ?? "/tmp/ts-self-contracts-out";
  const detDir = `/tmp/ts-self-determinism-${process.pid}`;

  console.log("== ProvekIt TypeScript self-contracts orchestrator ==");
  console.log(`output dir: ${outDir}`);

  let mintA: MintResult;
  let mintB: MintResult;
  try {
    rmSync(detDir, { recursive: true, force: true });
    mintA = runMintSelfContracts(detDir);
    mintB = runMintSelfContracts(outDir);
  } catch (e) {
    console.error(`ERROR: mint failed: ${(e as Error).message}`);
    if (e instanceof Error && e.stack) console.error(e.stack);
    return 1;
  }

  console.log("");
  console.log("authored:");
  for (const { label, count } of mintB.perSourceCounts) {
    console.log(
      `  ${label.padStart(22)}  ${String(count).padStart(2)} contracts`,
    );
  }
  console.log(
    `  ${"[ALL]".padStart(22)}  ${String(mintB.totalContracts).padStart(2)} contracts (TOTAL)`,
  );

  console.log("");
  console.log("minted:");
  console.log(`  .proof file:        ${mintB.path}`);
  console.log(`  bytes:              ${mintB.bytesLen}`);
  console.log(`  members:            ${mintB.memberCount}`);
  console.log(`  total contracts:    ${mintB.totalContracts}`);
  console.log(`  catalog CID:        ${mintB.cid}`);

  if (mintA.cid !== mintB.cid) {
    console.error("");
    console.error("ERROR: byte-determinism check FAILED:");
    console.error(`  run A CID: ${mintA.cid}`);
    console.error(`  run B CID: ${mintB.cid}`);
    rmSync(detDir, { recursive: true, force: true });
    return 2;
  }
  rmSync(detDir, { recursive: true, force: true });
  console.log("  determinism check:  OK (two runs produced identical CIDs)");
  console.log("");
  console.log("== done. self-application: live. ==");
  return 0;
}

// Direct invocation guard: if this file is the entry point, run main.
// In a vitest test context, the importer calls `runMintSelfContracts`
// or `main` directly without ever tripping this branch.
const isDirectInvoke =
  typeof process !== "undefined" &&
  Array.isArray(process.argv) &&
  process.argv[1] !== undefined &&
  process.argv[1].endsWith("mint-ts-self-contracts.mts");
if (isDirectInvoke) {
  process.exit(main(process.argv));
}
