import { describe, it, expect } from "vitest";
import { generateKeypair } from "../producerKeys/index.js";
import { mintContract } from "../claimEnvelope/index.js";
import {
  buildProofEnvelope,
  decodeProofEnvelope,
  verifyProofEnvelope,
} from "./index.js";
import { createHash, randomBytes } from "node:crypto";
void hash16;
function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function makeMember(name: string) {
  const { privateKey } = generateKeypair({ seed: randomBytes(32) });
  return mintContract({
    producedBy: "test@1",
    privateKey,
    contractName: name,
    pre: { kind: "atomic", name: "true", args: [] },
    authoring: { producerKind: "kit-author", author: "test@1" },
  });
}

describe("proofEnvelope", () => {
  it("buildProofEnvelope yields deterministic bytes (same input → same CID)", () => {
    const { privateKey } = generateKeypair({ seed: Buffer.alloc(32, 0x42) });
    const m1 = makeMember("alpha");
    const members = new Map([[m1.cid, m1]]);
    const declaredAt = "2026-04-30T00:00:00.000Z";

    const a = buildProofEnvelope({
      name: "test",
      version: "1",
      members,
      signerCid: "sha256:signer",
      signerPrivateKey: privateKey,
      declaredAt,
    });
    const b = buildProofEnvelope({
      name: "test",
      version: "1",
      members,
      signerCid: "sha256:signer",
      signerPrivateKey: privateKey,
      declaredAt,
    });
    expect(a.cid).toBe(b.cid);
    expect(Buffer.from(a.bytes).equals(Buffer.from(b.bytes))).toBe(true);
  });

  it("decodeProofEnvelope round-trips members", () => {
    const { privateKey } = generateKeypair({ seed: Buffer.alloc(32, 0x42) });
    const m1 = makeMember("alpha");
    const m2 = makeMember("beta");
    const members = new Map([[m1.cid, m1], [m2.cid, m2]]);

    const built = buildProofEnvelope({
      name: "test",
      version: "1",
      members,
      signerCid: "sha256:signer",
      signerPrivateKey: privateKey,
    });
    const decoded = decodeProofEnvelope(built.bytes);
    expect(decoded.kind).toBe("catalog");
    expect(decoded.name).toBe("test");
    expect(decoded.version).toBe("1");
    expect(decoded.members.size).toBe(2);
    expect(decoded.members.has(m1.cid)).toBe(true);
    expect(decoded.members.has(m2.cid)).toBe(true);
  });

  it("verifyProofEnvelope passes for a freshly built envelope", () => {
    const { privateKey, publicKey } = generateKeypair({ seed: Buffer.alloc(32, 0x42) });
    const m1 = makeMember("alpha");
    const members = new Map([[m1.cid, m1]]);

    const built = buildProofEnvelope({
      name: "test",
      version: "1",
      members,
      signerCid: "sha256:signer",
      signerPrivateKey: privateKey,
    });
    const result = verifyProofEnvelope(built.bytes, built.cid, publicKey);
    expect(result.errors).toEqual([]);
    expect(result.ok).toBe(true);
  });

  it("verifyProofEnvelope rejects on filename CID mismatch (rule 1)", () => {
    const { privateKey, publicKey } = generateKeypair({ seed: Buffer.alloc(32, 0x42) });
    const m1 = makeMember("alpha");
    const members = new Map([[m1.cid, m1]]);

    const built = buildProofEnvelope({
      name: "test",
      version: "1",
      members,
      signerCid: "sha256:signer",
      signerPrivateKey: privateKey,
    });
    const result = verifyProofEnvelope(built.bytes, "deadbeef".repeat(4), publicKey);
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toMatch(/rule 1/);
  });

  it("verifyProofEnvelope rejects on tampered member bytes (rule 2)", () => {
    const { privateKey, publicKey } = generateKeypair({ seed: Buffer.alloc(32, 0x42) });
    const m1 = makeMember("alpha");
    // Tamper: build with different content under the original CID.
    const m1Tampered = { ...m1, producedBy: "tampered" };
    const members = new Map([[m1.cid, m1Tampered]]);

    const built = buildProofEnvelope({
      name: "test",
      version: "1",
      members,
      signerCid: "sha256:signer",
      signerPrivateKey: privateKey,
    });
    const result = verifyProofEnvelope(built.bytes, built.cid, publicKey);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => /rule 2/.test(e))).toBe(true);
  });

  it("verifyProofEnvelope rejects on wrong public key (rule 3)", () => {
    const { privateKey } = generateKeypair({ seed: Buffer.alloc(32, 0x42) });
    const { publicKey: wrongKey } = generateKeypair({ seed: Buffer.alloc(32, 0x99) });
    const m1 = makeMember("alpha");
    const members = new Map([[m1.cid, m1]]);

    const built = buildProofEnvelope({
      name: "test",
      version: "1",
      members,
      signerCid: "sha256:signer",
      signerPrivateKey: privateKey,
    });
    const result = verifyProofEnvelope(built.bytes, built.cid, wrongKey);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => /rule 3/.test(e))).toBe(true);
  });
});
