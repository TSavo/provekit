/**
 * load-all-proofs — Stage 0/1 of the bridge enforcement workflow.
 *
 * Walks every `.proof` file in the project root (every package under
 * node_modules including scoped packages, plus the project root itself),
 * decodes each, and returns a unified
 * CID-keyed pool of all member mementos. All downstream stages do
 * hash lookups against this pool — no further file IO.
 *
 * Also indexes the bridge envelopes separately so the
 * enumerate-callsites stage can match Ctor names to bridges in O(1).
 *
 * Spec: protocol/specs/2026-04-30-proof-file-format.md (file walking +
 * trust-root verification)
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { createHash } from "node:crypto";
import type { Stage } from "../types.js";
import { decodeProofEnvelope } from "../../proofEnvelope/index.js";
import { computeEnvelopeCid } from "../../claimEnvelope/cid.js";
import type { ClaimEnvelope, BridgeEvidence } from "../../claimEnvelope/types.js";

export const LOAD_ALL_PROOFS_CAPABILITY = "load-all-proofs";

export interface LoadAllProofsInput {
  projectRoot: string;
}

export interface LoadAllProofsOutput {
  /** CID → memento envelope. Every member of every .proof file. */
  mementoPool: Record<string, ClaimEnvelope>;
  /** sourceSymbol (IR name) → bridge envelope. Index for callsite enumeration. */
  bridgesBySymbol: Record<string, ClaimEnvelope>;
  /** Per-file errors encountered during the walk (failed trust root, etc.). */
  errors: Array<{ proofFile: string; reason: string }>;
}

export interface MakeLoadAllProofsStageDeps {
  producerVersion?: string;
}

export function makeLoadAllProofsStage(
  deps: MakeLoadAllProofsStageDeps = {},
): Stage<LoadAllProofsInput, LoadAllProofsOutput> {
  const producedBy = deps.producerVersion ?? "loadAllProofs@v1";

  return {
    name: "loadAllProofs",
    producedBy,

    serializeInput(input) {
      return { projectRoot: input.projectRoot };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as LoadAllProofsOutput;
    },

    async run(input) {
      const mementoPool: Record<string, ClaimEnvelope> = {};
      const bridgesBySymbol: Record<string, ClaimEnvelope> = {};
      const errors: Array<{ proofFile: string; reason: string }> = [];

      const proofPaths = enumerateProofFiles(input.projectRoot);
      for (const proofPath of proofPaths) {
        let bytes: Buffer;
        try {
          bytes = readFileSync(proofPath);
        } catch (e) {
          errors.push({ proofFile: proofPath, reason: `read: ${(e as Error).message}` });
          continue;
        }
        // Trust-root check: filename CID matches bytes hash.
        const filename = proofPath.split("/").pop()!;
        const m = filename.match(/^([0-9a-f]+)\.proof$/);
        if (m) {
          const filenameCid = m[1]!;
          const derivedCid = createHash("sha256").update(bytes).digest("hex").slice(0, 32);
          if (derivedCid !== filenameCid) {
            errors.push({
              proofFile: proofPath,
              reason: `rule 1 (trust root): filename CID ${filenameCid} != content hash ${derivedCid}`,
            });
            continue;
          }
        }

        let catalog;
        try {
          catalog = decodeProofEnvelope(new Uint8Array(bytes));
        } catch (e) {
          errors.push({ proofFile: proofPath, reason: `decode: ${(e as Error).message}` });
          continue;
        }

        for (const [memberCid, memberBytes] of catalog.members) {
          let env: ClaimEnvelope;
          try {
            env = JSON.parse(Buffer.from(memberBytes).toString("utf8"));
          } catch (e) {
            errors.push({
              proofFile: proofPath,
              reason: `member ${memberCid.slice(0, 12)}…: parse: ${(e as Error).message}`,
            });
            continue;
          }
          // Verify the member's CID re-derives.
          const derived = computeEnvelopeCid(env);
          if (derived !== memberCid) {
            errors.push({
              proofFile: proofPath,
              reason: `member ${memberCid.slice(0, 12)}… bytes derive to ${derived}`,
            });
            continue;
          }
          mementoPool[memberCid] = env;
          if (env.evidence?.kind === "bridge") {
            const ev = env.evidence as BridgeEvidence;
            bridgesBySymbol[ev.body.sourceSymbol] = env;
          }
        }
      }

      return { mementoPool, bridgesBySymbol, errors };
    },
  };
}

/**
 * Enumerate every .proof file rooted at the project: the project root
 * itself plus each top-level package under node_modules (including
 * scoped packages one level deep). Excludes nested node_modules to
 * avoid combinatorial walks.
 */
function enumerateProofFiles(projectRoot: string): string[] {
  const out: string[] = [];

  // Project root's own *.proof files.
  if (existsSync(projectRoot)) {
    pushProofs(projectRoot, out);
  }

  const nodeModules = join(projectRoot, "node_modules");
  if (!existsSync(nodeModules)) return out;

  for (const entry of readdirSyncSafe(nodeModules)) {
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
      for (const sub of readdirSyncSafe(entryPath)) {
        pushProofs(join(entryPath, sub), out);
      }
    } else {
      pushProofs(entryPath, out);
    }
  }
  return out;
}

function pushProofs(dir: string, out: string[]): void {
  for (const f of readdirSyncSafe(dir)) {
    if (f.endsWith(".proof")) out.push(join(dir, f));
  }
}

function readdirSyncSafe(p: string): string[] {
  try {
    return readdirSync(p);
  } catch {
    return [];
  }
}
