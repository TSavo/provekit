#!/usr/bin/env -S npx tsx
// SPDX-License-Identifier: Apache-2.0
//
// mint-ts-self-contracts: TypeScript peer-implementation orchestrator.
//
// 1. Lifts the TypeScript kit's native self-contract sources through the
//    existing provekit-lift adapters (vitest and fast-check).
// 2. Mints each lifted contract as a signed memento under the foundation
//    key (test seed [0x42; 32]) and bundles into a single .proof envelope
//    whose filename IS its CID.
// 3. Mints twice into separate output directories and asserts byte-
//    deterministic CIDs.
//
// The repo's other tsx-driven binaries (`bin/provekit.cjs`,
// `bin/provekit-lift.cjs`) are currently broken on Node 25 because
// @ipld/dag-cbor is ESM-only and tsx's CJS bridge can't resolve it.
// Vitest's Vite loader handles ESM cleanly: that's the working
// invocation:
//
//   pnpm vitest run implementations/typescript/src/bin/mint-ts-self-contracts.test.ts
//
// That test exercises `runMintSelfContracts` below and prints the
// catalog CID + determinism status.

import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

import { contractCidFromArgs, computeContractSetCid } from "../claimEnvelope/cid.js";
import {
  defaultLiftOptions,
  liftPath,
  mintProof,
  type AdapterWarning,
  type ContractDecl,
  type LiftReport,
} from "../lift/index.js";

export const PRODUCED_BY = "@provekit/ts-self-contracts@1.0";
export const DECLARED_AT = "2026-04-30T18:00:00.000Z";

export interface NativeSelfContractSource {
  label: string;
  path: string;
}

const SELF_CONTRACT_SOURCES: NativeSelfContractSource[] = [
  {
    label: "cross-kit-bridges",
    path: "implementations/typescript/src/lift/cross-kit-bridges.self-contracts.test.ts",
  },
  {
    label: "typescript-kit",
    path: "implementations/typescript/src/self-contracts/typescript-kit.self-contracts.test.ts",
  },
];

interface LiftedSource {
  source: NativeSelfContractSource;
  report: LiftReport;
  contracts: ContractDecl[];
}

function liftAllNativeSelfContracts(): LiftedSource[] {
  const lifted: LiftedSource[] = [];
  for (const source of SELF_CONTRACT_SOURCES) {
    const report = liftPath(resolve(source.path));
    const warnings = adapterWarnings(report);
    if (report.parseErrors.length > 0) {
      const first = report.parseErrors[0]!;
      throw new Error(`native self-contract parse error in ${first.path}: ${first.message}`);
    }
    if (warnings.length > 0) {
      const first = warnings[0]!;
      throw new Error(
        `native self-contract skipped by ${first.adapter} in ${first.sourcePath}: ` +
          `${first.itemName}: ${first.reason}`,
      );
    }
    if (report.decls.length === 0) {
      throw new Error(`native self-contract source emitted zero contracts: ${source.path}`);
    }
    lifted.push({ source, report, contracts: report.decls });
  }
  return lifted;
}

function adapterWarnings(report: LiftReport): AdapterWarning[] {
  return report.adapterReports.flatMap((adapter) => adapter.warnings);
}

function contractSetCidFor(decls: ContractDecl[]): string {
  const contentCids = decls.map((d) =>
    contractCidFromArgs({
      producedBy: PRODUCED_BY,
      producedAt: DECLARED_AT,
      privateKey: Buffer.alloc(0),
      contractName: d.name,
      outBinding: d.outBinding,
      ...(d.pre !== undefined ? { pre: d.pre } : {}),
      ...(d.post !== undefined ? { post: d.post } : {}),
      ...(d.inv !== undefined ? { inv: d.inv } : {}),
      authoring: {
        producerKind: "lift",
        lifter: "typescript-kit.self-contracts",
        evidence:
          d.adapter === "fast-check" || d.adapter === "vitest-tests"
            ? "tests"
            : "types",
      },
    }),
  );
  return computeContractSetCid(contentCids);
}

export interface MintResult {
  sourceMode: "native-lift";
  cid: string;
  contractSetCid: string;
  bytesLen: number;
  path: string;
  memberCount: number;
  totalContracts: number;
  perSourceCounts: { label: string; path: string; count: number }[];
}

/**
 * Lift the native TS self-contract sources, mint them as signed mementos,
 * bundle into a `.proof`, write to `<outDir>/<full-cid>.proof`, return the
 * result.
 */
export function runMintSelfContracts(outDir: string): MintResult {
  mkdirSync(outDir, { recursive: true });

  const lifted = liftAllNativeSelfContracts();
  const decls = lifted.flatMap((source) => source.contracts);
  const perSourceCounts: { label: string; path: string; count: number }[] = [];
  for (const source of lifted) {
    perSourceCounts.push({
      label: source.source.label,
      path: source.source.path,
      count: source.contracts.length,
    });
  }

  const minted = mintProof(decls, defaultLiftOptions({
    producedBy: PRODUCED_BY,
    producedAt: DECLARED_AT,
    catalogName: "@provekit/ts-self-contracts",
    catalogVersion: "1.0.0",
    lifter: "typescript-kit.self-contracts",
    quiet: true,
  }));

  if (!minted.cid.startsWith("blake3-512:")) {
    throw new Error("internal: cid missing blake3-512 prefix");
  }
  const contractSetCid = contractSetCidFor(decls);
  const path = join(outDir, `${minted.cid}.proof`);
  writeFileSync(path, Buffer.from(minted.bytes));

  return {
    sourceMode: "native-lift",
    cid: minted.cid,
    contractSetCid,
    bytesLen: minted.bytes.length,
    path,
    memberCount: minted.memberCount,
    totalContracts: decls.length,
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
  console.log(`  contractSetCid:     ${mintB.contractSetCid}`);

  if (mintA.cid !== mintB.cid || mintA.contractSetCid !== mintB.contractSetCid) {
    console.error("");
    console.error("ERROR: byte-determinism check FAILED:");
    console.error(`  run A CID:              ${mintA.cid}`);
    console.error(`  run B CID:              ${mintB.cid}`);
    console.error(`  run A contractSetCid:   ${mintA.contractSetCid}`);
    console.error(`  run B contractSetCid:   ${mintB.contractSetCid}`);
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
