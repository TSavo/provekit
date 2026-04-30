/**
 * Pass 8: SHA-256 prefix-16 hash.
 *
 * Produces a 16-character hex string from the sha256 of the canonical
 * byte buffer. 16 hex chars = 64 bits of hash.
 *
 * Per spec:
 *   propertyHash = sha256(canonicalBytes)[:16 hex chars]
 *   bindingHash  = sha256(scopeBytes)[:16 hex chars]
 *
 * The sha256 is Node's `crypto.createHash("sha256")` over the UTF-8
 * encoded bytes of the canonical JSON serialization.
 */

import { createHash } from "crypto";

/**
 * Hash `bytes` with SHA-256 and return the first 16 hex characters.
 */
export function sha256Prefix16(bytes: Buffer): string {
  const digest = createHash("sha256").update(bytes).digest("hex");
  return digest.slice(0, 16);
}
