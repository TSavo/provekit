/**
 * Producer-signature key management — protocol-first surface.
 *
 * v1 minimal: ed25519 keypair generation only. The legacy persistence
 * layer (publishPublicKey / rotateKey / revokeKey via SQLite memento
 * store) has been removed; key publication is now a memento like any
 * other (mint a public-key-memento via the standard claim-envelope
 * flow and ship it in the kit's .proof catalog).
 *
 * Spec: protocol/specs/2026-04-29-universal-claim-envelope.md
 *       (§ "Producer-signature scheme (v1)")
 */

import {
  KeyObject,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
} from "node:crypto";

export interface ProducerKeypair {
  publicKey: KeyObject;
  privateKey: KeyObject;
}

/**
 * Standard pkcs8 prefix for an ed25519 private key carrying a 32-byte
 * raw seed (RFC 8410). Concatenating this with the seed produces a
 * 48-byte DER blob that `createPrivateKey({format:"der", type:"pkcs8"})`
 * accepts deterministically: same seed in, same key bytes out.
 */
const ED25519_PKCS8_PREFIX = Buffer.from(
  "302e020100300506032b657004220420",
  "hex",
);

/**
 * Generate an ed25519 keypair.
 *
 * Pass `options.seed` (exactly 32 bytes) for a deterministic keypair
 * (tests). Without a seed, generated via `generateKeyPairSync("ed25519")`.
 */
export function generateKeypair(options?: { seed?: Buffer }): ProducerKeypair {
  if (options?.seed !== undefined) {
    if (options.seed.length !== 32) {
      throw new Error(
        `seed must be exactly 32 bytes for ed25519, got ${options.seed.length}`,
      );
    }
    const pkcs8 = Buffer.concat([ED25519_PKCS8_PREFIX, options.seed]);
    const privateKey = createPrivateKey({
      key: pkcs8,
      format: "der",
      type: "pkcs8",
    });
    const publicKey = createPublicKey(privateKey);
    return { publicKey, privateKey };
  }
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  return { publicKey, privateKey };
}

/** v1.1.0 algorithm tag for ed25519 public keys. */
export const PUBKEY_ALGORITHM_TAG = "ed25519";
/** Self-identifying prefix attached to public-key strings. */
export const PUBKEY_PREFIX = PUBKEY_ALGORITHM_TAG + ":";

/**
 * Export a public key as base64 SPKI DER (raw payload only, no
 * algorithm tag). Prefer `publicKeyToSelfIdentifying` for any value
 * that crosses the protocol surface.
 */
export function publicKeyToBase64(key: KeyObject): string {
  return key.export({ format: "der", type: "spki" }).toString("base64");
}

/**
 * Export a public key as a self-identifying string per protocol v1.1.0:
 *   "ed25519:" + base64(SPKI DER)
 *
 * This is the only form that should appear in mementos or `.proof`
 * catalogs; verifiers dispatch on the prefix.
 */
export function publicKeyToSelfIdentifying(key: KeyObject): string {
  return PUBKEY_PREFIX + publicKeyToBase64(key);
}

/** Reconstruct a public KeyObject from a base64 SPKI DER string (no prefix). */
export function publicKeyFromBase64(b64: string): KeyObject {
  return createPublicKey({
    key: Buffer.from(b64, "base64"),
    format: "der",
    type: "spki",
  });
}

/**
 * Reconstruct a public KeyObject from a self-identifying string
 * (`"ed25519:<base64-spki-der>"`). Throws on unknown algorithm tags.
 */
export function publicKeyFromSelfIdentifying(s: string): KeyObject {
  const colon = s.indexOf(":");
  if (colon < 0) {
    throw new Error(`pubkey missing algorithm tag prefix: ${JSON.stringify(s)}`);
  }
  const tag = s.slice(0, colon);
  if (tag !== PUBKEY_ALGORITHM_TAG) {
    throw new Error(
      `unsupported pubkey algorithm tag ${JSON.stringify(tag)}; v1.1.0 supports only "ed25519"`,
    );
  }
  return publicKeyFromBase64(s.slice(colon + 1));
}
