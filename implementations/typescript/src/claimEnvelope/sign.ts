/**
 * ed25519 signing and verification for claim envelopes (protocol v1.1.0).
 *
 * Uses Node's built-in `node:crypto` ed25519 support (available since
 * Node 12). No external dependency.
 *
 * Signing scope: canonicalized envelope with `cid` and `producerSignature`
 * elided: same bytes used for CID construction.
 *
 * Signature encoding (v1.1.0): self-identifying
 *   `"ed25519:" + base64(raw_signature_bytes)`
 *
 * The algorithm tag is part of the signature value, not metadata about
 * it; verifiers dispatch on the prefix at verification time.
 */

import { sign, verify, KeyObject } from "node:crypto";
import { canonicalEncode } from "./canonicalize.js";
import { envelopeForHashing } from "./cid.js";
import type { ClaimEnvelope } from "./types.js";

type SignableEnvelope = Omit<ClaimEnvelope, "cid"> & { cid?: string };

/** v1.1.0 algorithm tag for signatures + public keys. */
export const SIGNATURE_ALGORITHM_TAG = "ed25519";

/** Self-identifying prefix attached to signatures and public keys. */
export const SIGNATURE_PREFIX = SIGNATURE_ALGORITHM_TAG + ":";

/** Regex matching any v1.1.0+ self-identifying signature or pubkey string. */
export const SELF_IDENTIFYING_SIG_RE = /^[a-z0-9]+:[A-Za-z0-9+/]+=*$/;

/**
 * Strip the algorithm prefix from a self-identifying signature/pubkey.
 * Throws if the prefix does not match a known algorithm tag.
 */
function stripSignaturePrefix(s: string): string {
  const colon = s.indexOf(":");
  if (colon < 0) {
    throw new Error(`signature missing algorithm tag prefix: ${JSON.stringify(s)}`);
  }
  const tag = s.slice(0, colon);
  if (tag !== SIGNATURE_ALGORITHM_TAG) {
    throw new Error(
      `unsupported signature algorithm tag ${JSON.stringify(tag)}; v1.1.0 supports only "ed25519"`,
    );
  }
  return s.slice(colon + 1);
}

/**
 * Sign an envelope with an ed25519 private key.
 *
 * @param envelope - The envelope to sign (need not yet have a `cid`).
 * @param privateKey - ed25519 private key as a KeyObject or DER/PEM Buffer/string.
 * @returns self-identifying signature string `"ed25519:<base64>"`.
 */
export function signEnvelope(
  envelope: SignableEnvelope,
  privateKey: KeyObject | Buffer | string,
): string {
  const input = envelopeForHashing(envelope);
  const bytes = canonicalEncode(input);
  // `sign(algorithm, data, key)`: algorithm is null for ed25519
  // because the algorithm is encoded in the key itself.
  const sig = sign(null, bytes, privateKey);
  return SIGNATURE_PREFIX + sig.toString("base64");
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
  let b64: string;
  try {
    b64 = stripSignaturePrefix(envelope.producerSignature);
  } catch {
    return false;
  }
  const sig = Buffer.from(b64, "base64");
  try {
    return verify(null, bytes, publicKey, sig);
  } catch {
    return false;
  }
}
