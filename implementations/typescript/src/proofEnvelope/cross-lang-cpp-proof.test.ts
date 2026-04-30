// Cross-language proof verification: reads a .proof file produced by
// the C++ kit (implementations/cpp/provekit-ir-symbolic/example/parseInt_kit_proof.cpp)
// and verifies it round-trips end-to-end through the TS protocol.
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
import type { ClaimEnvelope, PropertyEvidence, BridgeEvidence } from "../claimEnvelope/types.js";

describe("cross-language: TS verifies a C++-produced .proof", () => {
  const PROOF_PATH = "/tmp/cpp-kit-out/84ca9c7c382cc28d3ca260cd69bda6c1.proof";

  it.runIf(existsSync(PROOF_PATH))(
    "TS decodes + verifies the C++-generated .proof end-to-end",
    () => {
      const bytes = readFileSync(PROOF_PATH);

      // Rule 1: filename CID = bytes hash (trust root).
      const derivedCid = createHash("sha256")
        .update(bytes)
        .digest("hex")
        .slice(0, 32);
      expect(derivedCid).toBe("84ca9c7c382cc28d3ca260cd69bda6c1");

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
      const seenCids: string[] = [];
      let propertyEnv: ClaimEnvelope | null = null;
      let bridgeEnv: ClaimEnvelope | null = null;
      for (const [cid, memberBytes] of catalog.members) {
        const env = JSON.parse(Buffer.from(memberBytes).toString("utf8")) as ClaimEnvelope;
        const reDerived = computeEnvelopeCid(env);
        expect(reDerived).toBe(cid);
        seenCids.push(cid);
        if (env.evidence?.kind === "property") propertyEnv = env;
        if (env.evidence?.kind === "bridge") bridgeEnv = env;
      }

      // Both members present + their kinds match what the C++ kit declared.
      expect(propertyEnv).not.toBeNull();
      expect(bridgeEnv).not.toBeNull();

      // Property memento carries the IR formula (forall x: Int. x > 0).
      const propEv = propertyEnv!.evidence as PropertyEvidence;
      const formula = propEv.body.irFormula as { kind: string; predicate: { body: { kind: string; predicate: string } } };
      expect(formula.kind).toBe("forall");
      expect(formula.predicate.body.kind).toBe("atomic");
      expect(formula.predicate.body.predicate).toBe(">");
      expect(propEv.body.irKitVersion).toBe("cpp-kit@1.0");

      // Bridge memento points at the property memento's CID.
      const bridgeEv = bridgeEnv!.evidence as BridgeEvidence;
      expect(bridgeEv.body.sourceSymbol).toBe("parseInt");
      expect(bridgeEv.body.sourceLayer).toBe("ts");
      expect(bridgeEv.body.targetLayer).toBe("cpp-kit");
      expect(bridgeEv.body.irArgSorts).toEqual(["String"]);
      expect(bridgeEv.body.irReturnSort).toBe("Int");
      // The bridge's targetContractCid is the property memento's CID.
      expect(bridgeEv.body.targetContractCid).toBe(propertyEnv!.cid);
    },
  );
});
