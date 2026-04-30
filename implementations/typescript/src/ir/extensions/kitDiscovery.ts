/**
 * Kit discovery — walks a project's node_modules looking for packages
 * that ship a ProvekIt `.proof` file at their root, and registers the
 * bridges (and, in a future revision, extension declarations) carried
 * inside.
 *
 * Spec: protocol/specs/2026-04-30-proof-file-format.md
 *
 * Per-package shape: a package opts in by setting a `provekit` field
 * in its package.json. The field MAY include a `proofHash` hint
 * naming the `.proof` file; if absent, discovery falls back to an
 * extension scan at the package root and selects any *.proof file.
 *
 * Trust root: the file's filename CID equals its bytes hash. This
 * shape mirrors the protocol exactly — no language runtime is loaded;
 * verification is pure file IO + CBOR decode + SHA-256.
 *
 * Spec rules enforced by this walker:
 *   1. Filename CID matches content (rejects on mismatch)
 *   2. Each member envelope's CID matches its identity
 *
 * Spec rule deferred to a follow-up:
 *   3. Catalog signature (requires public-key memento walking; see TODO).
 *
 * Today's implementation registers BRIDGE envelopes only
 * (evidence.kind === "bridge"). Extension-declaration envelopes are not
 * yet a wire format (extensions sign separately today; task #41).
 * Other envelope variants are silently skipped pending dispatcher
 * implementations.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { listBridges, primitiveBridge } from "./bridges.js";
import type { PrimitiveBridgeDeclaration } from "./bridges.js";
import { registerExtensionDeclaration } from "./registry.js";
import type { ExtensionDeclaration, SortRef } from "./registry.js";
import { decodeProofEnvelope } from "../../proofEnvelope/index.js";
import { computeEnvelopeCid } from "../../claimEnvelope/cid.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";

export interface DiscoveredKit {
  packageName: string;
  packageVersion: string;
  packageRoot: string;
  /** Full path to the .proof file walked. */
  proofPath: string;
  /** Filename CID = bytes hash; the file's trust root. */
  proofCid: string;
  /** Bridge IR-names this package contributed. */
  registeredBridgeNames: string[];
  /** Verification errors encountered while walking. Empty = clean walk. */
  errors: string[];
}

export interface DiscoveryResult {
  kits: DiscoveredKit[];
  /** Bridge collisions detected during discovery. */
  collisions: BridgeCollision[];
  /** Bridges currently in the registry, keyed by IR name. */
  byName: Record<string, PrimitiveBridgeDeclaration>;
}

export interface BridgeCollision {
  irName: string;
  packages: Array<{ packageName: string; packageVersion: string; targetContractCid: string }>;
}

/**
 * Walk the project's node_modules looking for packages that ship a
 * `.proof` file; verify each file's trust root; decode embedded
 * member envelopes; register the bridges.
 */
export async function discoverProtocolKits(projectRoot: string): Promise<DiscoveryResult> {
  const kits: DiscoveredKit[] = [];
  const beforeNames = new Set(listBridges().map((b) => b.irName));

  const candidates = enumerateProtocolPackages(projectRoot);
  for (const cand of candidates) {
    const before = new Set(listBridges().map((b) => b.irName));
    const result = walkProofFile(cand);
    const after = new Set(listBridges().map((b) => b.irName));
    const newlyRegistered = [...after].filter((n) => !before.has(n));

    // Tag this package's bridges with provenance from package.json.
    for (const bridge of listBridges()) {
      if (newlyRegistered.includes(bridge.irName) && !bridge.registeredBy) {
        bridge.registeredBy = {
          packageName: cand.packageName,
          packageVersion: cand.packageVersion,
        };
      }
    }

    kits.push({
      packageName: cand.packageName,
      packageVersion: cand.packageVersion,
      packageRoot: cand.packageRoot,
      proofPath: result.proofPath,
      proofCid: result.proofCid,
      registeredBridgeNames: newlyRegistered,
      errors: result.errors,
    });
  }

  // Pre-existing bridges (registered in-process before discovery, e.g.
  // by an internal kit's lazy init) get a synthetic provenance tag so
  // the LSP hover renderer always has a "registered by" answer.
  for (const bridge of listBridges()) {
    if (beforeNames.has(bridge.irName) && !bridge.registeredBy) {
      bridge.registeredBy = {
        packageName: "(internal kit lazy-init)",
        packageVersion: "n/a",
      };
    }
  }

  const byName: Record<string, PrimitiveBridgeDeclaration> = {};
  const collisions: BridgeCollision[] = [];
  for (const bridge of listBridges()) {
    byName[bridge.irName] = bridge;
  }

  return { kits, collisions, byName };
}

interface ProtocolPackageCandidate {
  packageName: string;
  packageVersion: string;
  packageRoot: string;
  /** From package.json's provekit.proofHash if present; else null. */
  proofHashHint: string | null;
}

interface WalkResult {
  proofPath: string;
  proofCid: string;
  errors: string[];
}

function walkProofFile(cand: ProtocolPackageCandidate): WalkResult {
  // Discovery: hint first, extension scan fallback.
  let proofPath = "";
  if (cand.proofHashHint) {
    const hinted = join(cand.packageRoot, `${cand.proofHashHint}.proof`);
    if (existsSync(hinted)) proofPath = hinted;
  }
  if (!proofPath) {
    const proofs = readdirSync(cand.packageRoot).filter((f) => f.endsWith(".proof"));
    if (proofs.length > 0) proofPath = join(cand.packageRoot, proofs[0]!);
  }
  if (!proofPath) {
    return { proofPath: "", proofCid: "", errors: ["no .proof file at package root"] };
  }

  const filename = proofPath.split("/").pop()!;
  const m = filename.match(/^([0-9a-f]+)\.proof$/);
  const filenameCid = m ? m[1]! : null;

  const errors: string[] = [];
  let bytes: Buffer;
  try {
    bytes = readFileSync(proofPath);
  } catch (e) {
    return {
      proofPath,
      proofCid: "",
      errors: [`cannot read .proof: ${(e as Error).message}`],
    };
  }
  const derivedCid = createHash("sha256").update(bytes).digest("hex").slice(0, 32);

  // Spec rule 1: filename matches content.
  if (filenameCid === null) {
    errors.push(`filename "${filename}" does not match <cid>.proof pattern`);
  } else if (filenameCid !== derivedCid) {
    errors.push(
      `rule 1 (trust root): filename CID ${filenameCid} != content hash ${derivedCid}`,
    );
    return { proofPath, proofCid: derivedCid, errors };
  }

  let catalog;
  try {
    catalog = decodeProofEnvelope(new Uint8Array(bytes));
  } catch (e) {
    errors.push(`decode: ${(e as Error).message}`);
    return { proofPath, proofCid: derivedCid, errors };
  }

  // Spec rule 2: each member's CID matches its envelope identity.
  // Dispatch on evidence.kind to register what we know how to.
  for (const [cid, memberBytes] of catalog.members) {
    let env: ClaimEnvelope;
    try {
      env = JSON.parse(Buffer.from(memberBytes).toString("utf8")) as ClaimEnvelope;
    } catch (e) {
      errors.push(`member ${cid.slice(0, 12)}…: failed to parse: ${(e as Error).message}`);
      continue;
    }
    const derived = computeEnvelopeCid(env);
    if (derived !== cid) {
      errors.push(`rule 2: member ${cid.slice(0, 12)}… bytes derive to ${derived}`);
      continue;
    }

    if (env.evidence?.kind === "bridge") {
      registerBridgeFromEnvelope(env);
    } else if (env.evidence?.kind === "extension-declaration") {
      registerExtensionFromEnvelope(env);
    }
    // Other envelope variants (property, deprecation, public-key, etc.)
    // are silently skipped pending dispatcher implementations.
  }

  // Spec rule 3 (signature) deferred — requires public-key memento walking.

  return { proofPath, proofCid: derivedCid, errors };
}

function registerBridgeFromEnvelope(env: ClaimEnvelope): void {
  const body = (env.evidence as { body: Record<string, unknown> }).body;
  const sourceSymbol = String(body.sourceSymbol);
  const sourceLayer = String(body.sourceLayer);
  const targetContractCid = String(body.targetContractCid);
  const targetLayer = String(body.targetLayer);
  const notes = typeof body.notes === "string" ? body.notes : undefined;

  // Type signature now carried natively in the envelope (task #40):
  // irArgSorts is an array of SortRef, irReturnSort is a SortRef.
  // Both required per the bridge envelope schema.
  const irArgSorts = (Array.isArray(body.irArgSorts) ? body.irArgSorts : []) as SortRef[];
  const irReturnSort = (body.irReturnSort ?? "Int") as SortRef;

  primitiveBridge({
    irName: sourceSymbol,
    irArgSorts,
    irReturnSort,
    sourceLayer,
    targetContractCid,
    targetLayer,
    ...(notes !== undefined ? { notes } : {}),
  });
}

function registerExtensionFromEnvelope(env: ClaimEnvelope): void {
  const body = (env.evidence as { body: { declaration: unknown } }).body;
  const declaration = body.declaration as ExtensionDeclaration;
  registerExtensionDeclaration(declaration);
}

/**
 * Walk node_modules and enumerate packages whose package.json carries
 * a `provekit` field. Returns each candidate with the package metadata
 * and any proofHash hint.
 */
function enumerateProtocolPackages(projectRoot: string): ProtocolPackageCandidate[] {
  const out: ProtocolPackageCandidate[] = [];
  const nodeModules = join(projectRoot, "node_modules");
  if (!existsSync(nodeModules)) return out;

  for (const entry of readdirSync(nodeModules)) {
    if (entry.startsWith(".")) continue;
    const entryPath = join(nodeModules, entry);
    let entryStat;
    try {
      entryStat = statSync(entryPath);
    } catch {
      continue;
    }
    if (!entryStat.isDirectory()) continue;

    if (entry.startsWith("@")) {
      let scopedEntries: string[];
      try {
        scopedEntries = readdirSync(entryPath);
      } catch {
        continue;
      }
      for (const sub of scopedEntries) {
        const cand = inspectPackage(join(entryPath, sub));
        if (cand) out.push(cand);
      }
    } else {
      const cand = inspectPackage(entryPath);
      if (cand) out.push(cand);
    }
  }
  return out;
}

function inspectPackage(packageRoot: string): ProtocolPackageCandidate | null {
  const pkgJsonPath = join(packageRoot, "package.json");
  if (!existsSync(pkgJsonPath)) return null;
  let pkg: { name?: string; version?: string; provekit?: unknown };
  try {
    pkg = JSON.parse(readFileSync(pkgJsonPath, "utf-8"));
  } catch {
    return null;
  }
  if (!pkg.provekit || typeof pkg.provekit !== "object") return null;
  if (!pkg.name || !pkg.version) return null;

  const provekitField = pkg.provekit as Record<string, unknown>;
  const proofHashHint =
    typeof provekitField.proofHash === "string" ? provekitField.proofHash : null;

  return {
    packageName: pkg.name,
    packageVersion: pkg.version,
    packageRoot,
    proofHashHint,
  };
}
