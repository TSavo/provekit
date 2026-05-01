// Cross-language proof verification: reads any v1.1.0 .proof file
// produced by a peer-language kit (C++ / Go / Rust) and verifies it
// round-trips end-to-end through the TS protocol v1.1.
//
// This is the empirical proof of cross-language correctness: the peer
// kit ships a .proof file; the TS verifier consumes it; the trust
// chain holds at every layer (filename CID, member CIDs, signatures,
// IR formula extraction).
//
// Discovers proofs in /tmp/{cpp,go,rust}-kit-out-v11/ and round-trips
// each one as its own subtest. The catalog name + authoring author
// are matched against the publishing-kit family (cpp / go / rust).

import { describe, it, expect } from "vitest";
import { existsSync, readdirSync, readFileSync } from "fs";
import { decodeProofEnvelope } from "./index.js";
import { computeEnvelopeCid } from "../claimEnvelope/cid.js";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import { computeCid } from "../canonicalizer/hash.js";
import type {
  ClaimEnvelope,
  ContractEvidence,
  BridgeEvidence,
} from "../claimEnvelope/types.js";

/**
 * Discover all v1.1.0-shaped peer-kit `.proof` files. v1.1.0 filenames
 * are `"<algorithm>-<bits>:<hex>.proof"` (e.g.
 * `"blake3-512:af13...3a.proof"`). One subtest per discovered file so
 * each cell of the four-way matrix reports independently.
 */
const PEER_DIRS: { dir: string; family: "cpp" | "go" | "rust" }[] = [
  { dir: "/tmp/cpp-kit-out-v11", family: "cpp" },
  { dir: "/tmp/go-kit-out-v11", family: "go" },
  { dir: "/tmp/rust-kit-out-v11", family: "rust" },
];
const PROOF_FILENAME_RE = /^([a-z0-9]+-[0-9]+:[0-9a-f]+)\.proof$/;

interface PeerProof {
  path: string;
  cid: string;
  family: "cpp" | "go" | "rust";
}

function findAllPeerProofs(): PeerProof[] {
  const out: PeerProof[] = [];
  for (const { dir, family } of PEER_DIRS) {
    if (!existsSync(dir)) continue;
    for (const entry of readdirSync(dir)) {
      const m = entry.match(PROOF_FILENAME_RE);
      if (m) out.push({ path: `${dir}/${entry}`, cid: m[1]!, family });
    }
  }
  return out;
}

describe("cross-language: TS verifies peer-published .proofs", () => {
  const proofs = findAllPeerProofs();

  // Top-level skip: never run when no peer artifacts exist.
  it.runIf(proofs.length === 0)("(skipped) no peer .proof files discovered", () => {
    // pass-through
  });

  for (const proof of proofs) {
    it(`TS decodes + verifies the ${proof.family}-generated .proof end-to-end (${proof.cid.slice(0, 24)}...)`, () => {
      const { path: PROOF_PATH, cid: PROOF_FILENAME_CID, family } = proof;
      const bytes = readFileSync(PROOF_PATH);

      // Rule 1: filename CID = bytes hash (trust root).
      const derivedCid = computeCid(bytes);
      expect(derivedCid).toBe(PROOF_FILENAME_CID);

      // Decode the catalog envelope (deterministic CBOR, RFC 8949 §4.2.1).
      const catalog = decodeProofEnvelope(new Uint8Array(bytes));
      expect(catalog.kind).toBe("catalog");
      expect(catalog.name).toBe(`@example/${family}-kit`);
      expect(catalog.version).toBe("1.0.0");
      // Signer CID is self-identifying ("ed25519:..." or "blake3-512:...").
      expect(catalog.signer).toMatch(/^[a-z0-9]+(?:-[0-9]+)?:.+$/);
      expect(catalog.declaredAt).toMatch(/^2026-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
      expect(catalog.signature.length).toBe(64); // ed25519 sig

      // Rule 2: every embedded member's CID re-derives correctly.
      expect(catalog.members.size).toBe(2);
      let contractEnv: ClaimEnvelope | null = null;
      let bridgeEnv: ClaimEnvelope | null = null;
      for (const [cid, memberBytes] of catalog.members) {
        const env = JSON.parse(Buffer.from(memberBytes).toString("utf8")) as ClaimEnvelope;
        const reDerived = computeEnvelopeCid(env);
        expect(reDerived).toBe(cid);
        if (env.evidence?.kind === "contract") contractEnv = env;
        if (env.evidence?.kind === "bridge") bridgeEnv = env;
      }

      // Both members present + their kinds match what the peer kit declared.
      expect(contractEnv).not.toBeNull();
      expect(bridgeEnv).not.toBeNull();

      // Contract memento carries the precondition formula (forall n: Int. n > 0).
      const contractEv = contractEnv!.evidence as ContractEvidence;
      expect(contractEv.body.contractName).toBe("parseInt");
      expect(contractEv.body.outBinding).toBe("out");
      const pre = contractEv.body.pre as {
        kind: string;
        name: string;
        sort: { kind: string; name: string };
        body: { kind: string; name: string };
      };
      expect(pre.kind).toBe("forall");
      expect(pre.body.kind).toBe("atomic");
      expect(pre.body.name).toBe(">");
      expect(contractEv.body.preHash).toMatch(/^blake3-512:[0-9a-f]{128}$/);
      // Cross-language hash agreement: TS recomputes preHash from the
      // peer-published formula and gets the same bytes. Catches any drift
      // in JCS canonical encoding between TS and the peer.
      expect(computeCid(canonicalEncode(contractEv.body.pre))).toBe(
        contractEv.body.preHash,
      );
      // Authoring is a kit-author block; author matches the peer family.
      const authoring = contractEv.body.authoring as { producerKind: string; author: string };
      expect(authoring.producerKind).toBe("kit-author");
      expect(authoring.author).toBe(`${family}-kit@1.0`);

      // Bridge memento points at the contract memento's CID.
      const bridgeEv = bridgeEnv!.evidence as BridgeEvidence;
      expect(bridgeEv.body.sourceSymbol).toBe("parseInt");
      expect(bridgeEv.body.sourceLayer).toBe("ts");
      expect(bridgeEv.body.targetLayer).toBe(`${family}-kit`);
      expect(bridgeEv.body.irArgSorts).toEqual(["String"]);
      expect(bridgeEv.body.irReturnSort).toBe("Int");
      expect(bridgeEv.body.targetContractCid).toBe(contractEnv!.cid);
    });
  }
});
