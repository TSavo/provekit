/**
 * Tests for src/producerKeys/index.ts.
 *
 * Spec: docs/specs/2026-04-29-next-wave-prompts.md (Prompt 3)
 *       docs/specs/2026-04-29-universal-claim-envelope.md
 *         (§ "Producer-signature scheme (v1)")
 *
 * Coverage:
 *   1. generateKeypair returns KeyObjects (random + deterministic seed).
 *   2. publishPublicKey writes a memento, returns a CID, sets keypair.publicKeyCid.
 *   3. loadProducerKey resolves the published key.
 *   4. signMemento + verifyMemento roundtrip on an envelope.
 *   5. rotateKey: loadProducerKey returns the NEW key; verifyMemento
 *      reports keyRotationDepth: 1.
 *   6. revokeKey: loadProducerKey returns null; verifyMemento returns
 *      {valid: false, revoked: true}.
 *   7. Tampered signature: byte-flip → valid: false.
 *   8. Unknown producer: loadProducerKey throws.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { KeyObject } from "node:crypto";
import { openDb } from "../db/index.js";
import {
  computeEnvelopeCid,
  type ClaimEnvelope,
  type EvidenceVariant,
} from "../claimEnvelope/index.js";
import { VARIANT_SCHEMA_CIDS } from "../claimEnvelope/variants/index.js";
import {
  generateKeypair,
  publishPublicKey,
  loadProducerKey,
  signMemento,
  verifyMemento,
  rotateKey,
  revokeKey,
  type ProducerKeypair,
  type SignableEnvelope,
} from "./index.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "producer-keys-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const PRODUCER = "z3-symbolic@4.13.4";

function makeLegacyEvidence(): EvidenceVariant {
  return {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"],
    body: {
      rawWitness: '{"z3":"sat"}',
      legacyProducerId: PRODUCER,
    },
  };
}

function makeUnsignedEnvelope(
  producer = PRODUCER,
): SignableEnvelope {
  return {
    schemaVersion: "1",
    bindingHash: "a1b2c3d4e5f6a7b8",
    propertyHash: "b2c3d4e5f6a7b8c9",
    verdict: "holds",
    producedBy: producer,
    producedAt: "2026-04-29T12:00:00Z",
    inputCids: [],
    evidence: makeLegacyEvidence(),
  };
}

function finalize(env: SignableEnvelope, signature: string): ClaimEnvelope {
  // Same canonical form `signEnvelope` used → CID is stable regardless of
  // whether we add the signature first or last.
  const cid = computeEnvelopeCid(env);
  return { ...env, producerSignature: signature, cid } as ClaimEnvelope;
}

describe("generateKeypair", () => {
  it("returns KeyObjects without a seed", () => {
    const k = generateKeypair();
    expect(k.publicKey).toBeInstanceOf(KeyObject);
    expect(k.privateKey).toBeInstanceOf(KeyObject);
    expect(k.publicKey.type).toBe("public");
    expect(k.privateKey.type).toBe("private");
    expect(k.publicKeyCid).toBe("");
  });

  it("produces deterministic keypairs from a seed", () => {
    const seed = Buffer.alloc(32, 0x42);
    const a = generateKeypair({ seed });
    const b = generateKeypair({ seed });
    const aPub = a.publicKey.export({ format: "der", type: "spki" });
    const bPub = b.publicKey.export({ format: "der", type: "spki" });
    expect(aPub.equals(bPub)).toBe(true);
  });

  it("rejects a wrong-length seed", () => {
    expect(() => generateKeypair({ seed: Buffer.alloc(16) })).toThrow(
      /seed must be exactly 32 bytes/,
    );
  });
});

describe("publishPublicKey + loadProducerKey", () => {
  it("publishes a key memento and resolves it back", () => {
    const db = makeDb();
    const kp = generateKeypair({ seed: Buffer.alloc(32, 0x01) });
    const cid = publishPublicKey(PRODUCER, kp, db);

    expect(cid).toMatch(/^[0-9a-f]{32}$/);
    expect(kp.publicKeyCid).toBe(cid);

    const loaded = loadProducerKey(PRODUCER, db);
    expect(loaded).not.toBeNull();
    const original = kp.publicKey.export({ format: "der", type: "spki" });
    const back = loaded!.export({ format: "der", type: "spki" });
    expect(original.equals(back as Buffer)).toBe(true);
  });

  it("throws for an unknown producer", () => {
    const db = makeDb();
    expect(() => loadProducerKey("nobody@v1", db)).toThrow(
      /no public key found/,
    );
  });
});

describe("signMemento + verifyMemento", () => {
  it("round-trips: signed envelope verifies against the published key", () => {
    const db = makeDb();
    const kp = generateKeypair({ seed: Buffer.alloc(32, 0x10) });
    publishPublicKey(PRODUCER, kp, db);

    const env = makeUnsignedEnvelope();
    const sig = signMemento(env, kp.privateKey);
    const signed = finalize(env, sig);

    const result = verifyMemento(signed, db);
    expect(result.valid).toBe(true);
    expect(result.keyCid).toBe(kp.publicKeyCid);
    expect(result.keyRotationDepth).toBe(0);
    expect(result.revoked).toBeUndefined();
  });

  it("rejects a tampered signature", () => {
    const db = makeDb();
    const kp = generateKeypair({ seed: Buffer.alloc(32, 0x20) });
    publishPublicKey(PRODUCER, kp, db);

    const env = makeUnsignedEnvelope();
    const sig = signMemento(env, kp.privateKey);
    const sigBytes = Buffer.from(sig, "base64");
    sigBytes[0] ^= 0x01; // flip one bit
    const tampered = finalize(env, sigBytes.toString("base64"));

    const result = verifyMemento(tampered, db);
    expect(result.valid).toBe(false);
    expect(result.reason).toMatch(/signature verification failed/);
  });

  it("rejects an envelope with no signature", () => {
    const db = makeDb();
    const kp = generateKeypair({ seed: Buffer.alloc(32, 0x30) });
    publishPublicKey(PRODUCER, kp, db);

    const env = makeUnsignedEnvelope();
    const cid = computeEnvelopeCid(env);
    const unsigned = { ...env, cid } as ClaimEnvelope;

    const result = verifyMemento(unsigned, db);
    expect(result.valid).toBe(false);
    expect(result.reason).toMatch(/no producerSignature/);
  });

  it("rejects when the producer has no published key", () => {
    const db = makeDb();
    const kp = generateKeypair({ seed: Buffer.alloc(32, 0x40) });
    // Note: NOT publishing.

    const env = makeUnsignedEnvelope();
    const sig = signMemento(env, kp.privateKey);
    const signed = finalize(env, sig);

    const result = verifyMemento(signed, db);
    expect(result.valid).toBe(false);
    expect(result.reason).toMatch(/no public key found/);
  });
});

describe("rotateKey", () => {
  it("loadProducerKey returns the new key after rotation", () => {
    const db = makeDb();
    const oldKp = generateKeypair({ seed: Buffer.alloc(32, 0xa1) });
    publishPublicKey(PRODUCER, oldKp, db);

    const newKp = generateKeypair({ seed: Buffer.alloc(32, 0xa2) });
    rotateKey(PRODUCER, oldKp, newKp, db);

    const loaded = loadProducerKey(PRODUCER, db);
    expect(loaded).not.toBeNull();
    const expected = newKp.publicKey.export({ format: "der", type: "spki" });
    const got = loaded!.export({ format: "der", type: "spki" });
    expect(expected.equals(got as Buffer)).toBe(true);
  });

  it("verifyMemento reports keyRotationDepth=1 for a memento signed with the rotated key", () => {
    const db = makeDb();
    const oldKp = generateKeypair({ seed: Buffer.alloc(32, 0xb1) });
    publishPublicKey(PRODUCER, oldKp, db);

    const newKp = generateKeypair({ seed: Buffer.alloc(32, 0xb2) });
    rotateKey(PRODUCER, oldKp, newKp, db);

    const env = makeUnsignedEnvelope();
    const sig = signMemento(env, newKp.privateKey);
    const signed = finalize(env, sig);

    const result = verifyMemento(signed, db);
    expect(result.valid).toBe(true);
    expect(result.keyRotationDepth).toBe(1);
    expect(result.keyCid).toBe(newKp.publicKeyCid);
  });

  it("walks a multi-step rotation chain (depth=2)", () => {
    const db = makeDb();
    const k0 = generateKeypair({ seed: Buffer.alloc(32, 0xc1) });
    publishPublicKey(PRODUCER, k0, db);

    const k1 = generateKeypair({ seed: Buffer.alloc(32, 0xc2) });
    rotateKey(PRODUCER, k0, k1, db);

    const k2 = generateKeypair({ seed: Buffer.alloc(32, 0xc3) });
    rotateKey(PRODUCER, k1, k2, db);

    const env = makeUnsignedEnvelope();
    const sig = signMemento(env, k2.privateKey);
    const signed = finalize(env, sig);

    const result = verifyMemento(signed, db);
    expect(result.valid).toBe(true);
    expect(result.keyRotationDepth).toBe(2);
    expect(result.keyCid).toBe(k2.publicKeyCid);
  });

  it("throws when rotating from an unpublished old keypair", () => {
    const db = makeDb();
    const oldKp = generateKeypair({ seed: Buffer.alloc(32, 0xd1) });
    const newKp = generateKeypair({ seed: Buffer.alloc(32, 0xd2) });
    expect(() => rotateKey(PRODUCER, oldKp, newKp, db)).toThrow(
      /publishPublicKey must run first/,
    );
  });
});

describe("revokeKey", () => {
  it("loadProducerKey returns null after the current key is revoked", () => {
    const db = makeDb();
    const kp = generateKeypair({ seed: Buffer.alloc(32, 0xe1) });
    publishPublicKey(PRODUCER, kp, db);

    revokeKey(PRODUCER, kp, kp, db, "compromised");

    expect(loadProducerKey(PRODUCER, db)).toBeNull();
  });

  it("verifyMemento returns {valid: false, revoked: true} after revocation", () => {
    const db = makeDb();
    const kp = generateKeypair({ seed: Buffer.alloc(32, 0xf1) });
    publishPublicKey(PRODUCER, kp, db);

    const env = makeUnsignedEnvelope();
    const sig = signMemento(env, kp.privateKey);
    const signed = finalize(env, sig);

    revokeKey(PRODUCER, kp, kp, db, "leak");

    const result = verifyMemento(signed, db);
    expect(result.valid).toBe(false);
    expect(result.revoked).toBe(true);
    expect(result.keyCid).toBe(kp.publicKeyCid);
  });

  it("revoking after rotation revokes the rotated tip, not the original", () => {
    const db = makeDb();
    const k0 = generateKeypair({ seed: Buffer.alloc(32, 0x71) });
    publishPublicKey(PRODUCER, k0, db);

    const k1 = generateKeypair({ seed: Buffer.alloc(32, 0x72) });
    rotateKey(PRODUCER, k0, k1, db);

    revokeKey(PRODUCER, k1, k1, db, "compromised");

    expect(loadProducerKey(PRODUCER, db)).toBeNull();
  });
});

describe("integration sanity", () => {
  it("publish + sign + verify works across producers without cross-talk", () => {
    const db = makeDb();
    const kpA = generateKeypair({ seed: Buffer.alloc(32, 0xaa) });
    const kpB = generateKeypair({ seed: Buffer.alloc(32, 0xbb) });
    publishPublicKey("producer-a@v1", kpA, db);
    publishPublicKey("producer-b@v1", kpB, db);

    const envA = makeUnsignedEnvelope("producer-a@v1");
    const sigA = signMemento(envA, kpA.privateKey);
    const signedA = finalize(envA, sigA);

    // Try to verify A's envelope claiming it came from B — must fail.
    const forgery: ClaimEnvelope = {
      ...signedA,
      producedBy: "producer-b@v1",
    };
    const cidForgery = computeEnvelopeCid(forgery);
    const forgeryFinal = { ...forgery, cid: cidForgery };
    const result = verifyMemento(forgeryFinal, db);
    expect(result.valid).toBe(false);
  });
});

// type-check: make sure exported types are reachable
type _AssertExport = ProducerKeypair;
