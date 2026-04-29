/**
 * CID construction for claim envelopes.
 *
 * Spec: docs/specs/2026-04-29-universal-claim-envelope.md §CID construction
 *
 * cid = sha256(canonicalize(envelope_without_cid_and_signature))[:32 hex chars]
 *
 * The fields `cid` and `producerSignature` are ELIDED (not set to null/undefined,
 * just omitted from the input object) before canonicalization. This avoids
 * self-reference and signature dependency.
 */

import { createHash } from "node:crypto";
import { canonicalEncode } from "./canonicalize.js";
import type { ClaimEnvelope } from "./types.js";

/**
 * Build the canonical input object for CID/signature computation.
 * Strips `cid` and `producerSignature`; sorts `inputCids` per spec.
 */
export function envelopeForHashing(
  envelope: Omit<ClaimEnvelope, "cid"> & { cid?: string },
): Record<string, unknown> {
  // Construct with only the specified fields in a stable order.
  // keys will be re-sorted by canonicalEncode regardless, but
  // explicit construction makes it clear what is included.
  const {
    schemaVersion,
    bindingHash,
    propertyHash,
    verdict,
    producedBy,
    producedAt,
    inputCids,
    evidence,
  } = envelope as ClaimEnvelope;

  return {
    schemaVersion,
    bindingHash,
    propertyHash,
    verdict,
    producedBy,
    producedAt,
    inputCids: [...inputCids].sort(),
    evidence,
  };
}

/**
 * Compute the CID for a claim envelope.
 *
 * sha256-prefix-32 (32 hex chars = first 128 bits of sha256) of the
 * canonicalized envelope with `cid` and `producerSignature` elided.
 */
export function computeEnvelopeCid(
  envelope: Omit<ClaimEnvelope, "cid"> & { cid?: string },
): string {
  const input = envelopeForHashing(envelope);
  const bytes = canonicalEncode(input);
  return createHash("sha256").update(bytes).digest("hex").slice(0, 32);
}
