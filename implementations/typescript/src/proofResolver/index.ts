/**
 * Contract precondition resolver — given a contract memento CID, walk
 * all locally-available `.proof` files in a project's node_modules and
 * return the matching member envelope's `pre` formula. Callers walking
 * bridge targets use this to discharge call-site obligations.
 *
 * Pure file IO + CBOR decode + envelope lookup. No network. No solver.
 * No SMT. The IR formula is returned untouched.
 *
 * Spec: protocol/specs/2026-04-30-proof-file-format.md (file walk)
 *       protocol/specs/2026-04-30-memento-envelope-grammar.md (contract role)
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { decodeProofEnvelope } from "../proofEnvelope/index.js";
import { computeEnvelopeCid } from "../claimEnvelope/cid.js";
import type { ClaimEnvelope, ContractEvidence } from "../claimEnvelope/types.js";
import type { IrFormula } from "../ir/formulas.js";

export interface ResolvedProperty {
  cid: string;
  /** Precondition formula extracted from the contract memento's `pre` slot. */
  irFormula: IrFormula;
  contractName: string;
  outBinding: string;
  /** Source `.proof` filename the member was found in. */
  proofFile: string;
  /** Package the .proof file belongs to. */
  packageName: string;
}

/**
 * Walk node_modules for any `.proof` file, decode each, look for the
 * member envelope whose CID matches `cid`. If found AND its evidence
 * is a contract variant with a non-empty `pre` formula, return the
 * resolved precondition.
 *
 * Returns null if no matching member exists, the matched member is not
 * a contract variant, or the contract has no precondition.
 */
export function resolvePropertyFormula(
  projectRoot: string,
  cid: string,
): ResolvedProperty | null {
  const member = findMemberByCid(projectRoot, cid);
  if (!member) return null;
  if (member.envelope.evidence.kind !== "contract") return null;
  const ev = member.envelope.evidence as ContractEvidence;
  const pre = ev.body.pre as IrFormula | undefined;
  if (pre === undefined) return null;
  return {
    cid,
    irFormula: pre,
    contractName: ev.body.contractName,
    outBinding: ev.body.outBinding,
    proofFile: member.proofFile,
    packageName: member.packageName,
  };
}

interface FoundMember {
  envelope: ClaimEnvelope;
  proofFile: string;
  packageName: string;
}

function findMemberByCid(projectRoot: string, cid: string): FoundMember | null {
  const nodeModules = join(projectRoot, "node_modules");
  if (!existsSync(nodeModules)) return null;

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

    const candidates = entry.startsWith("@")
      ? readdirSyncSafe(entryPath).map((sub) => join(entryPath, sub))
      : [entryPath];

    for (const packageRoot of candidates) {
      const member = findMemberInPackage(packageRoot, cid);
      if (member) return member;
    }
  }
  return null;
}

function findMemberInPackage(packageRoot: string, cid: string): FoundMember | null {
  if (!existsSync(packageRoot)) return null;
  let entries: string[];
  try {
    entries = readdirSync(packageRoot);
  } catch {
    return null;
  }
  const proofs = entries.filter((f) => f.endsWith(".proof"));
  if (proofs.length === 0) return null;

  let packageName = packageRoot.split("/").slice(-2).join("/");
  try {
    const pkg = JSON.parse(readFileSync(join(packageRoot, "package.json"), "utf-8"));
    if (typeof pkg.name === "string") packageName = pkg.name;
  } catch {
    // Use directory-derived name as fallback.
  }

  for (const proof of proofs) {
    const proofPath = join(packageRoot, proof);
    let bytes: Buffer;
    try {
      bytes = readFileSync(proofPath);
    } catch {
      continue;
    }
    let catalog;
    try {
      catalog = decodeProofEnvelope(new Uint8Array(bytes));
    } catch {
      continue;
    }
    const memberBytes = catalog.members.get(cid);
    if (!memberBytes) continue;
    let envelope: ClaimEnvelope;
    try {
      envelope = JSON.parse(Buffer.from(memberBytes).toString("utf8"));
    } catch {
      continue;
    }
    // Verify the member's CID re-derives to the requested cid (defense
    // against tampered bundles where a member's bytes don't match its key).
    const derived = computeEnvelopeCid(envelope);
    if (derived !== cid) continue;
    return { envelope, proofFile: proof, packageName };
  }
  return null;
}

function readdirSyncSafe(p: string): string[] {
  try {
    return readdirSync(p);
  } catch {
    return [];
  }
}
