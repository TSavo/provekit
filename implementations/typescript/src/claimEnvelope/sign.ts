/**
 * ed25519 signing and verification for claim envelopes.
 *
 * Uses Node's built-in `node:crypto` ed25519 support (available since
 * Node 12). No external dependency.
 *
 * Signing scope: canonicalized envelope with `cid` and `producerSignature`
 * elided — same bytes used for CID construction.
 *
 * Signature encoding: base64 string stored in `producerSignature`.
 */

import { sign, verify, KeyObject } from "node:crypto";
import { canonicalEncode } from "./canonicalize.js";
import { envelopeForHashing } from "./cid.js";
import type { ClaimEnvelope } from "./types.js";

type SignableEnvelope = Omit<ClaimEnvelope, "cid"> & { cid?: string };

/**
 * Sign an envelope with an ed25519 private key.
 *
 * @param envelope - The envelope to sign (need not yet have a `cid`).
 * @param privateKey - ed25519 private key as a KeyObject or DER/PEM Buffer/string.
 * @returns base64-encoded ed25519 signature.
 */
export function signEnvelope(
  envelope: SignableEnvelope,
  privateKey: KeyObject | Buffer | string,
): string {
  const input = envelopeForHashing(envelope);
  const bytes = canonicalEncode(input);
  // `sign(algorithm, data, key)` — algorithm is null for ed25519
  // because the algorithm is encoded in the key itself.
  const sig = sign(null, bytes, privateKey);
  return sig.toString("base64");
}

/**
 * Verify an envelope's ed25519 signature.
 *
 * @param envelope - The envelope including the `producerSignature` field.
 * @param publicKey - ed25519 public key as a KeyObject or DER/PEM Buffer/string.
 * @returns true if the signature is valid, false otherwise.
 */
export function verifyEnvelopeSignature(
  envelope: SignableEnvelope & { producerSignature?: string },
  publicKey: KeyObject | Buffer | string,
): boolean {
  if (!envelope.producerSignature) return false;
  const input = envelopeForHashing(envelope);
  const bytes = canonicalEncode(input);
  const sig = Buffer.from(envelope.producerSignature, "base64");
  try {
    return verify(null, bytes, publicKey, sig);
  } catch {
    return false;
  }
}
