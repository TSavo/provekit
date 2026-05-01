/**
 * Pass 8: BLAKE3-512 self-identifying hash (protocol v1.1.0).
 *
 * Spec: protocol/specs/2026-04-30-canonicalization-grammar.md §11
 *       protocol/specs/2026-04-30-memento-envelope-grammar.md §"Self-identifying"
 *
 * Every hash that crosses the protocol surface carries its algorithm tag
 * inline:
 *
 *   <algorithm>-<bits>:<lowercase-hex-digest>
 *
 * v1.1.0 ships with `blake3-512` (full 64-byte / 128 hex BLAKE3 digest) as
 * the only permitted hash tag. There is no truncation. There are no
 * per-purpose lengths.
 *
 *   propertyHash = "blake3-512:" + hex(BLAKE3_512(canonicalBytes))
 *   bindingHash  = "blake3-512:" + hex(BLAKE3_512(scopeBytes))
 *   cid          = "blake3-512:" + hex(BLAKE3_512(envelope_without_cid_and_signature))
 */

import { blake3 } from "@noble/hashes/blake3.js";
import { bytesToHex } from "@noble/hashes/utils.js";

/** The algorithm tag for v1.1.0. */
export const HASH_ALGORITHM_TAG = "blake3-512";

/** The full self-identifying prefix, e.g. "blake3-512:". */
export const HASH_PREFIX = HASH_ALGORITHM_TAG + ":";

/** Regex matching any v1.1.0+ self-identifying hash string. */
export const SELF_IDENTIFYING_HASH_RE = /^[a-z0-9]+-[0-9]+:[0-9a-f]+$/;

/**
 * Compute BLAKE3 over `bytes` and return the full 128-character lowercase
 * hex digest (64-byte / 512-bit output). NOT prefixed.
 */
export function blake3_512_hex(bytes: Buffer | Uint8Array): string {
  const out = blake3(bytes, { dkLen: 64 });
  return bytesToHex(out);
}

/**
 * Compute the self-identifying CID for canonical bytes:
 *   "blake3-512:" + blake3_512_hex(canonical_bytes)
 *
 * Used for every hash field in the protocol: bindingHash, propertyHash,
 * preHash, postHash, invHash, antecedentHash, consequentHash, member CIDs.
 */
export function computeCid(bytes: Buffer | Uint8Array): string {
  return HASH_PREFIX + blake3_512_hex(bytes);
}
