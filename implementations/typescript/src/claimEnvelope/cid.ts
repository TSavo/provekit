/**
 * CID construction for claim envelopes (protocol v1.1.0).
 *
 * Spec: protocol/specs/2026-04-29-universal-claim-envelope.md §CID construction
 *       protocol/specs/2026-04-30-memento-envelope-grammar.md §"Self-identifying"
 *
 * cid = "blake3-512:" + hex(BLAKE3_512(canonicalize(envelope_without_cid_and_signature)))
 *
 * The fields `cid` and `producerSignature` are ELIDED (not set to null/undefined,
 * just omitted from the input object) before canonicalization. This avoids
 * self-reference and signature dependency. The resulting hash carries the
 * algorithm tag inline so verifiers dispatch on the prefix.
 */

import { canonicalEncode } from "./canonicalize.js";
import { computeCid } from "../canonicalizer/hash.js";
import type { ClaimEnvelope } from "./types.js";
import type { MintContractArgs } from "./mint.js";

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
 * Returns the self-identifying string
 * `"blake3-512:" + hex(BLAKE3_512(canonical_bytes))`.
 */
export function computeEnvelopeCid(
  envelope: Omit<ClaimEnvelope, "cid"> & { cid?: string },
): string {
  const input = envelopeForHashing(envelope);
  const bytes = canonicalEncode(input);
  return computeCid(bytes);
}

/**
 * Compute the signer-independent contractCid for a contract.
 *
 * Per spec 2026-05-03-contract-cid-vs-attestation-cid.md §1:
 *   contractCid = blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
 *
 * Two distinct signers attesting the same logical contract produce the
 * same contractCid. This is NOT the attestation CID (envelope hash).
 */
export function contractCidFromArgs(args: MintContractArgs): string {
  const obj: Record<string, unknown> = {
    name: args.contractName,
    outBinding: args.outBinding ?? "out",
  };
  if (args.pre !== undefined) obj["pre"] = args.pre;
  if (args.post !== undefined) obj["post"] = args.post;
  if (args.inv !== undefined) obj["inv"] = args.inv;
  return computeCid(canonicalEncode(obj));
}

/**
 * Compute the contractSetCid from a list of signer-independent
 * contractCid strings (each "blake3-512:<128 hex>").
 *
 * Per spec 2026-05-03-contract-set-extension.md §1:
 *   contractSetCid = blake3-512(JCS(<sorted contractCIDs>))
 *
 * Sort is lexicographic; two kits enumerating the same contracts in
 * different order produce byte-identical contractSetCid values.
 */
export function computeContractSetCid(contractCids: string[]): string {
  const sorted = [...contractCids].sort();
  return computeCid(canonicalEncode(sorted));
}
