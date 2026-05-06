/**
 * provekit-lift (TypeScript) — workspace lift toolchain.
 *
 * Walks a TS project, parses every `.ts` file with the TypeScript
 * Compiler API, dispatches each parsed source to all registered
 * adapters, collects ContractDecls, mints them via
 * src/claimEnvelope/mint.ts, and bundles the result into a single
 * signed `.proof` catalog file whose filename IS the bytes hash (CID).
 *
 * STRATEGIC POSITIONING (read this before extending):
 *
 *   ProvekIt does NOT compete with `zod`, `fast-check`, `io-ts`, `yup`,
 *   `joi`, `class-validator`, `valibot`, etc. It sits BENEATH them.
 *   Developers keep their existing schema/property library; the lift
 *   adapters in this directory READ what's already there and promote
 *   each construct to a content-addressed signed contract memento.
 *
 *   The `proveLift/` LLM-driven pipeline is a fallback for greenfield
 *   code where no annotation library is in use.
 *
 * Mirrors the Rust `provekit-lift` crate one-for-one. See
 * implementations/rust/provekit-lift/README.md for the strategic frame.
 */

import { readFileSync, readdirSync, statSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve, extname } from "node:path";
import { KeyObject } from "node:crypto";
import ts from "typescript";

import { generateKeypair } from "../producerKeys/index.js";
import { mintContract } from "../claimEnvelope/mint.js";
import type { ClaimEnvelope } from "../claimEnvelope/types.js";
import { buildProofEnvelope } from "../proofEnvelope/index.js";
import { computeCid } from "../canonicalizer/hash.js";

import { liftFile as liftZodFile }            from "./adapters/zod.js";
import { liftFile as liftFastCheckFile }      from "./adapters/fast-check.js";
import { liftFile as liftClassValidatorFile } from "./adapters/class-validator.js";
import { liftFile as liftVitestTestsFile }    from "./adapters/vitest-tests.js";
import { liftFile as liftProvekitAnnotationsFile } from "./adapters/provekit-annotations.js";
import type { ContractDecl, LiftReport, AdapterReport, CallEdgeDecl } from "./types.js";

export type {
  ContractDecl,
  LiftReport,
  AdapterReport,
  AdapterOutput,
  AdapterWarning,
  CallEdgeDecl,
} from "./types.js";

export { liftZodFile, liftFastCheckFile, liftClassValidatorFile, liftVitestTestsFile };

/** Default ed25519 dev seed: same value as Rust (`[0x42; 32]`) for cross-impl
 * CID determinism in fixtures. */
export const DEFAULT_LIFT_SEED = Buffer.alloc(32, 0x42);

export interface LiftOptions {
  /** Producer identity recorded in each minted memento. */
  producedBy: string;
  /** ISO-8601 timestamp; default `2026-04-30T00:00:00.000Z` for reproducibility. */
  producedAt: string;
  /** ed25519 32-byte seed; default DEFAULT_LIFT_SEED. */
  signerSeed: Buffer;
  /** Catalog name written into the .proof envelope. */
  catalogName: string;
  /** Catalog version string. */
  catalogVersion: string;
  /** "Lifter" identity recorded in each contract's authoring block. */
  lifter: string;
  /** Suppress duplicate-contract warnings on stderr. */
  quiet?: boolean;
}

export function defaultLiftOptions(overrides: Partial<LiftOptions> = {}): LiftOptions {
  return {
    producedBy: "provekit-lift-ts@0.1.0",
    producedAt: "2026-04-30T00:00:00.000Z",
    signerSeed: DEFAULT_LIFT_SEED,
    catalogName: "@provekit/lift",
    catalogVersion: "0.1.0",
    lifter: "provekit-lift-ts",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// File walking
// ---------------------------------------------------------------------------

const SKIP_DIRS = new Set([
  "node_modules",
  ".git",
  "dist",
  "build",
  "target",
  "out",
  ".next",
  ".turbo",
  ".vite",
  "coverage",
]);

function enumerateTsFiles(root: string): string[] {
  const out: string[] = [];
  const walk = (dir: string): void => {
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      return;
    }
    for (const e of entries) {
      const p = join(dir, e);
      let st;
      try {
        st = statSync(p);
      } catch {
        continue;
      }
      if (st.isDirectory()) {
        if (SKIP_DIRS.has(e)) continue;
        if (e.startsWith(".")) continue;
        walk(p);
      } else if (st.isFile()) {
        const ext = extname(e);
        if (ext === ".ts" || ext === ".tsx" || ext === ".mts" || ext === ".cts") {
          // Skip declaration files — no value, no annotations to lift.
          if (e.endsWith(".d.ts")) continue;
          out.push(p);
        }
      }
    }
  };
  let st;
  try {
    st = statSync(root);
  } catch {
    return out;
  }
  if (st.isFile()) return [root];
  walk(root);
  return out;
}

// ---------------------------------------------------------------------------
// Lift
// ---------------------------------------------------------------------------

export function liftPath(root: string): LiftReport {
  const decls: ContractDecl[] = [];
  const adapterReports: Record<string, AdapterReport> = {
    zod: { adapter: "zod", seen: 0, lifted: 0, warnings: [] },
    "fast-check": { adapter: "fast-check", seen: 0, lifted: 0, warnings: [] },
    "class-validator": { adapter: "class-validator", seen: 0, lifted: 0, warnings: [] },
    "vitest-tests": { adapter: "vitest-tests", seen: 0, lifted: 0, warnings: [] },
    "provekit-annotations": { adapter: "provekit-annotations", seen: 0, lifted: 0, warnings: [] },
  };
  let filesScanned = 0;
  const parseErrors: Array<{ path: string; message: string }> = [];

  for (const filePath of enumerateTsFiles(root)) {
    filesScanned += 1;
    let src: string;
    try {
      src = readFileSync(filePath, "utf8");
    } catch (e) {
      parseErrors.push({ path: filePath, message: `read: ${(e as Error).message}` });
      continue;
    }
    const sf = ts.createSourceFile(filePath, src, ts.ScriptTarget.ES2022, true);

    const z = liftZodFile(sf, filePath);
    adapterReports.zod!.seen += z.seen;
    adapterReports.zod!.lifted += z.lifted;
    adapterReports.zod!.warnings.push(...z.warnings);
    decls.push(...z.decls);

    const f = liftFastCheckFile(sf, filePath);
    adapterReports["fast-check"]!.seen += f.seen;
    adapterReports["fast-check"]!.lifted += f.lifted;
    adapterReports["fast-check"]!.warnings.push(...f.warnings);
    decls.push(...f.decls);

    const cv = liftClassValidatorFile(sf, filePath);
    adapterReports["class-validator"]!.seen += cv.seen;
    adapterReports["class-validator"]!.lifted += cv.lifted;
    adapterReports["class-validator"]!.warnings.push(...cv.warnings);
    decls.push(...cv.decls);

    const vt = liftVitestTestsFile(sf, filePath);
    adapterReports["vitest-tests"]!.seen += vt.seen;
    adapterReports["vitest-tests"]!.lifted += vt.lifted;
    adapterReports["vitest-tests"]!.warnings.push(...vt.warnings);
    decls.push(...vt.decls);

    const pka = liftProvekitAnnotationsFile(sf, filePath);
    adapterReports["provekit-annotations"]!.seen += pka.seen;
    adapterReports["provekit-annotations"]!.lifted += pka.lifted;
    adapterReports["provekit-annotations"]!.warnings.push(...pka.warnings);
    decls.push(...pka.decls);
  }

  return {
    decls,
    adapterReports: [
      adapterReports.zod!,
      adapterReports["fast-check"]!,
      adapterReports["class-validator"]!,
      adapterReports["vitest-tests"]!,
      adapterReports["provekit-annotations"]!,
    ],
    filesScanned,
    parseErrors,
  };
}

// ---------------------------------------------------------------------------
// Mint + bundle
// ---------------------------------------------------------------------------

export interface MintOutput {
  bytes: Uint8Array;
  cid: string;
  memberCount: number;
  /** Map from contract name -> minted memento CID. */
  contractCids: Record<string, string>;
  /** Number of decls collapsed by content-addressed dedup. */
  deduplicated: number;
  /** Call edges for annotation-driven cross-kit bridges. */
  callEdges: CallEdgeDecl[];
}

export class NameCollisionDifferentIrError extends Error {
  constructor(public contractName: string) {
    super(
      `name collision on different IR: contract "${contractName}" lifted twice with different bodies`,
    );
    this.name = "NameCollisionDifferentIrError";
  }
}

export function mintProof(decls: ContractDecl[], opts: LiftOptions): MintOutput {
  const { privateKey, publicKey } = generateKeypair({ seed: opts.signerSeed });

  const members = new Map<string, ClaimEnvelope>();
  const contractCids: Record<string, string> = {};
  let deduplicated = 0;

  for (const d of decls) {
    const env = mintContract({
      producedBy: opts.producedBy,
      producedAt: opts.producedAt,
      privateKey,
      contractName: d.name,
      outBinding: d.outBinding,
      ...(d.pre !== undefined ? { pre: d.pre } : {}),
      ...(d.post !== undefined ? { post: d.post } : {}),
      ...(d.inv !== undefined ? { inv: d.inv } : {}),
      authoring: {
        producerKind: "lift",
        lifter: opts.lifter,
        // The protocol enum is constrained to {tests, types, docs, symbolic-exec}.
        // zod schemas express types; fast-check properties express tests.
        evidence:
          d.adapter === "fast-check" || d.adapter === "vitest-tests" ? "tests" : "types",
      },
    });

    const prev = contractCids[d.name];
    if (prev !== undefined) {
      if (prev === env.cid) {
        deduplicated += 1;
        continue;
      }
      // Skip duplicate with different body — log warning but don't fail
      if (!opts.quiet) {
        process.stderr.write(
          `provekit-lift: warn: skipping duplicate contract "${d.name}" with different body (from ${d.sourcePath})\n`,
        );
      }
      continue;
    }
    contractCids[d.name] = env.cid;

    if (members.has(env.cid)) {
      deduplicated += 1;
    } else {
      members.set(env.cid, env);
    }
  }

  // Signer CID: BLAKE3-512 self-identifying hash of the SPKI-DER pubkey.
  const pubDer = publicKey.export({ type: "spki", format: "der" }) as Buffer;
  const signerCid = computeCid(pubDer);

  const built = buildProofEnvelope({
    name: opts.catalogName,
    version: opts.catalogVersion,
    members,
    signerCid,
    signerPrivateKey: privateKey,
    declaredAt: opts.producedAt,
  });

  // Generate call edges for annotation-driven cross-kit bridges
  const callEdges: CallEdgeDecl[] = [];
  for (const d of decls) {
    if (!d.targetContract || !d.sourceLine) continue;
    if (!contractCids[d.name]) continue;
    callEdges.push({
      sourceContractCid: contractCids[d.name]!,
      targetContractCid: null,
      targetSymbol: d.targetContract,
      callSiteLocus: { file: d.sourcePath, line: d.sourceLine, col: 0 },
      evidenceTerm: { kind: "atomic", name: "true", args: [] },
    });
  }

  return {
    bytes: built.bytes,
    cid: built.cid,
    memberCount: members.size,
    contractCids,
    deduplicated,
    callEdges,
  };
}

/** Convenience: walk -> lift -> mint -> write `<outDir>/<cid>.proof`. */
export function liftAndMint(
  workspaceRoot: string,
  outDir: string,
  opts: LiftOptions = defaultLiftOptions(),
): { report: LiftReport; minted: MintOutput; outPath: string } {
  const report = liftPath(workspaceRoot);
  if (report.decls.length === 0) {
    throw new Error("no liftable contracts found in workspace");
  }
  const minted = mintProof(report.decls, opts);
  mkdirSync(outDir, { recursive: true });
  const outPath = join(resolve(outDir), `${minted.cid}.proof`);
  writeFileSync(outPath, Buffer.from(minted.bytes));
  return { report, minted, outPath };
}

/** Recover the public key for a given lift seed (used by verifier helpers). */
export function publicKeyForSeed(seed: Buffer = DEFAULT_LIFT_SEED): KeyObject {
  return generateKeypair({ seed }).publicKey;
}
