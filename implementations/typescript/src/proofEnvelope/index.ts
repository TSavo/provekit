/**
 * .proof file format: protocol-level binary distribution artifact.
 *
 * Spec: protocol/specs/2026-04-30-proof-file-format.md
 *
 * Build, encode, decode, and verify .proof envelopes. The envelope is
 * a deterministic CBOR (RFC 8949 §4.2.1) encoding of a catalog memento
 * with member envelopes embedded as opaque byte strings. The filename
 * IS the bytes hash (CID), and that hash is the protocol's trust root.
 */

import { sign as cryptoSign, verify as cryptoVerify, KeyObject } from "node:crypto";
import { encode as cborEncode, decode as cborDecode } from "@ipld/dag-cbor";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import { computeEnvelopeCid } from "../claimEnvelope/cid.js";
import { computeCid } from "../canonicalizer/hash.js";
import type { ClaimEnvelope } from "../claimEnvelope/types.js";

export interface ProofEnvelopeInput {
  name: string;
  version: string;
  /** Map from member CID (envelope CID per memento envelope grammar) to the full signed ClaimEnvelope. */
  members: Map<string, ClaimEnvelope>;
  /** CID of a public-key memento (must itself be a member, or otherwise resolvable by the verifier). */
  signerCid: string;
  signerPrivateKey: KeyObject;
  /** ISO 8601 timestamp; defaults to now. */
  declaredAt?: string;
  /** Other .proof file CIDs this catalog references transitively. */
  dependsOn?: string[];
  /**
   * Optional binary attestation back-pin. Self-identifying CID
   * (`"blake3-512:<hex>"`) of the compiled binary this proof bundle
   * covers. When present, a verifier MUST reject the bundle if the
   * running binary's hash differs. Spec:
   * protocol/specs/2026-04-30-proof-file-format.md (the supply-chain
   * anchor rule). Included in the signed payload, so tampering with
   * the pin breaks the catalog signature.
   */
  binaryCid?: string;
}

export interface ProofEnvelope {
  /** CBOR-encoded bytes; hash equals `cid`. */
  bytes: Uint8Array;
  /**
   * Self-identifying CID of the bytes
   * (`"blake3-512:" + hex(BLAKE3_512(bytes))`); matches the `.proof`
   * filename without extension.
   */
  cid: string;
}

export interface DecodedProofCatalog {
  kind: "catalog";
  name: string;
  version: string;
  /** Embedded member bytes by CID. Each value is the canonical bytes of a signed ClaimEnvelope. */
  members: Map<string, Uint8Array>;
  signer: string;
  signature: Uint8Array;
  declaredAt: string;
  dependsOn?: string[];
  /**
   * Optional binary attestation back-pin recovered from the catalog.
   * Set iff the original `buildProofEnvelope` input provided one.
   * Verifiers that care about supply-chain integrity MUST compare
   * against the running binary's hash. See proof-file-format spec.
   */
  binaryCid?: string;
}

export interface VerifyResult {
  ok: boolean;
  errors: string[];
  /** Decoded catalog (present even on partial failure for inspection). */
  catalog: DecodedProofCatalog | null;
  /** The CID derived from the bytes; compare to filename. */
  derivedCid: string;
}

/**
 * Build a .proof envelope.
 *
 * Returns the deterministic-CBOR-encoded bytes plus the bytes' CID
 * (which is the .proof filename without extension).
 */
export function buildProofEnvelope(input: ProofEnvelopeInput): ProofEnvelope {
  const memberBytes = new Map<string, Uint8Array>();
  for (const [cid, env] of input.members) {
    memberBytes.set(cid, new Uint8Array(canonicalEncode(env)));
  }

  const declaredAt = input.declaredAt ?? new Date().toISOString();

  // dag-cbor sorts map keys; encoding a plain object with sorted-on-construct
  // keys is sufficient. We convert Map → plain object for `members` so
  // the wire shape is a CBOR map, not a CBOR tag-extended Map.
  const membersObj: Record<string, Uint8Array> = {};
  for (const [k, v] of memberBytes) membersObj[k] = v;

  const unsignedBody: Record<string, unknown> = {
    declaredAt,
    kind: "catalog",
    members: membersObj,
    name: input.name,
    signer: input.signerCid,
    version: input.version,
  };
  if (input.dependsOn && input.dependsOn.length > 0) {
    unsignedBody.dependsOn = [...input.dependsOn].sort();
  }
  // Per proof-file-format spec: binaryCid is OPTIONAL and, when set,
  // is part of the signed payload (tampering with it must invalidate
  // the catalog signature). Key sorting is handled by dag-cbor.
  if (input.binaryCid !== undefined) {
    unsignedBody.binaryCid = input.binaryCid;
  }

  const unsignedBytes = cborEncode(unsignedBody);
  const sigBuf = cryptoSign(null, Buffer.from(unsignedBytes), input.signerPrivateKey);

  const signedBody = { ...unsignedBody, signature: new Uint8Array(sigBuf) };
  const bytes = cborEncode(signedBody);

  // Filename CID: full BLAKE3-512 self-identifying hash, no truncation.
  const cid = computeCid(Buffer.from(bytes));
  return { bytes, cid };
}

/** Decode a .proof envelope's bytes. Throws on CBOR or shape errors. */
export function decodeProofEnvelope(bytes: Uint8Array): DecodedProofCatalog {
  const decoded = cborDecode(bytes) as Record<string, unknown>;
  if (decoded.kind !== "catalog") {
    throw new Error(`expected kind="catalog", got ${JSON.stringify(decoded.kind)}`);
  }
  const membersRaw = decoded.members as Record<string, Uint8Array>;
  const members = new Map<string, Uint8Array>();
  for (const k of Object.keys(membersRaw)) {
    members.set(k, membersRaw[k]!);
  }
  const out: DecodedProofCatalog = {
    kind: "catalog",
    name: String(decoded.name),
    version: String(decoded.version),
    members,
    signer: String(decoded.signer),
    signature: decoded.signature as Uint8Array,
    declaredAt: String(decoded.declaredAt),
  };
  if (decoded.dependsOn) out.dependsOn = decoded.dependsOn as string[];
  if (typeof decoded.binaryCid === "string") {
    out.binaryCid = decoded.binaryCid;
  }
  return out;
}

/**
 * Verify a .proof envelope's integrity against the spec's rules:
 *   1. Filename matches content (caller passes filename CID; we recompute).
 *   2. Embedded member CIDs match envelope identities.
 *   3. Catalog signature is valid.
 *   (Member signatures are checked by the caller when needed; this
 *   function does the envelope-level integrity gate.)
 *
 * Per spec §3 rule 4, package.json hint mismatches are NOT a verification
 * failure; that is the caller's discovery concern.
 */
export function verifyProofEnvelope(
  bytes: Uint8Array,
  filenameCid: string,
  signerPublicKey: KeyObject,
): VerifyResult {
  const errors: string[] = [];
  const derivedCid = computeCid(Buffer.from(bytes));

  // Rule 1: filename matches content (trust root).
  if (derivedCid !== filenameCid) {
    errors.push(`rule 1: filename CID ${filenameCid} != content hash ${derivedCid}`);
    return { ok: false, errors, catalog: null, derivedCid };
  }

  let catalog: DecodedProofCatalog;
  try {
    catalog = decodeProofEnvelope(bytes);
  } catch (e) {
    errors.push(`decode: ${(e as Error).message}`);
    return { ok: false, errors, catalog: null, derivedCid };
  }

  // Rule 2: embedded member CIDs match envelope identities.
  for (const [cid, memberBytes] of catalog.members) {
    let env: ClaimEnvelope;
    try {
      env = JSON.parse(Buffer.from(memberBytes).toString("utf8"));
    } catch (e) {
      errors.push(`member ${cid}: failed to parse as JSON envelope: ${(e as Error).message}`);
      continue;
    }
    const derived = computeEnvelopeCid(env);
    if (derived !== cid) {
      errors.push(`rule 2: member ${cid} bytes hash to ${derived}`);
    }
  }

  // Rule 3: catalog signature is valid.
  const unsignedBody: Record<string, unknown> = {
    declaredAt: catalog.declaredAt,
    kind: catalog.kind,
    members: Object.fromEntries(catalog.members),
    name: catalog.name,
    signer: catalog.signer,
    version: catalog.version,
  };
  if (catalog.dependsOn) unsignedBody.dependsOn = catalog.dependsOn;
  // Mirror the build path: include binaryCid in the signing payload
  // when present, so tampered pins fail signature verification.
  if (catalog.binaryCid !== undefined) {
    unsignedBody.binaryCid = catalog.binaryCid;
  }
  const signingPayload = cborEncode(unsignedBody);
  const sigOk = cryptoVerify(null, Buffer.from(signingPayload), signerPublicKey, Buffer.from(catalog.signature));
  if (!sigOk) {
    errors.push("rule 3: catalog signature invalid for declared signer");
  }

  return { ok: errors.length === 0, errors, catalog, derivedCid };
}
