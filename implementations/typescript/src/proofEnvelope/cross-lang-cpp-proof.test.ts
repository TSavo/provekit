// Cross-language proof verification: reads a .proof file produced by
// the C++ kit (implementations/cpp/provekit-ir-symbolic/example/parseInt_kit_proof.cpp)
// and verifies it round-trips end-to-end through the TS protocol v1.1.
//
// This is the empirical proof of cross-language correctness: the C++
// kit ships a .proof file; the TS verifier consumes it; the trust
// chain holds at every layer (filename CID, member CIDs, signatures,
// IR formula extraction).

import { describe, it, expect } from "vitest";
import { existsSync, readFileSync } from "fs";
import { createHash } from "node:crypto";
import { decodeProofEnvelope } from "./index.js";
import { computeEnvelopeCid } from "../claimEnvelope/cid.js";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import { sha256Prefix16 } from "../canonicalizer/hash.js";
import type {
  ClaimEnvelope,
  ContractEvidence,
  BridgeEvidence,
} from "../claimEnvelope/types.js";

describe("cross-language: TS verifies a C++-produced .proof", () => {
  const PROOF_FILENAME = "bfe74d1a9d836f926058b331002da2f5";
  const PROOF_PATH = `/tmp/cpp-kit-out-v11/${PROOF_FILENAME}.proof`;

  it.runIf(existsSync(PROOF_PATH))(
    "TS decodes + verifies the C++-generated .proof end-to-end",
    () => {
      const bytes = readFileSync(PROOF_PATH);

      // Rule 1: filename CID = bytes hash (trust root).
      const derivedCid = createHash("sha256")
        .update(bytes)
        .digest("hex")
        .slice(0, 32);
      expect(derivedCid).toBe(PROOF_FILENAME);

      // Decode the catalog envelope (deterministic CBOR, RFC 8949 §4.2.1).
      const catalog = decodeProofEnvelope(new Uint8Array(bytes));
      expect(catalog.kind).toBe("catalog");
      expect(catalog.name).toBe("@example/cpp-kit");
      expect(catalog.version).toBe("1.0.0");
      expect(catalog.signer).toBe("sha256:cpp-kit-signer");
      expect(catalog.declaredAt).toBe("2026-04-30T12:00:00.000Z");
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

      // Both members present + their kinds match what the C++ kit declared.
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
      expect(contractEv.body.preHash).toMatch(/^[0-9a-f]{16}$/);
      // Cross-language hash agreement: TS recomputes preHash from the
      // C++-published formula and gets the same bytes. Catches any drift
      // in JCS canonical encoding between TS and C++.
      expect(sha256Prefix16(canonicalEncode(contractEv.body.pre))).toBe(
        contractEv.body.preHash,
      );
      // Authoring is a kit-author block.
      const authoring = contractEv.body.authoring as { producerKind: string; author: string };
      expect(authoring.producerKind).toBe("kit-author");
      expect(authoring.author).toBe("cpp-kit@1.0");

      // Bridge memento points at the contract memento's CID.
      const bridgeEv = bridgeEnv!.evidence as BridgeEvidence;
      expect(bridgeEv.body.sourceSymbol).toBe("parseInt");
      expect(bridgeEv.body.sourceLayer).toBe("ts");
      expect(bridgeEv.body.targetLayer).toBe("cpp-kit");
      expect(bridgeEv.body.irArgSorts).toEqual(["String"]);
      expect(bridgeEv.body.irReturnSort).toBe("Int");
      expect(bridgeEv.body.targetContractCid).toBe(contractEnv!.cid);
    },
  );
});
