/**
 * Producer-signature key management for the ProvekIt framework.
 *
 * Spec: protocol/specs/2026-04-29-universal-claim-envelope.md
 *       (§ "Producer-signature scheme (v1)")
 *
 * What this module owns:
 *   - ed25519 keypair generation (deterministic from a seed for tests).
 *   - Public-key publication as content-addressed mementos.
 *   - Signature application to claim envelopes (thin wrapper around
 *     `signEnvelope` from src/claimEnvelope/sign.ts).
 *   - Signature verification: walks the rotation chain, honours
 *     revocation mementos, returns provenance.
 *   - Key rotation and revocation as memento patterns.
 *
 * Trust model: producer signs every memento it emits. Consumers
 * resolve the producer's current public key by walking the rotation
 * chain rooted at the original published key. A revocation memento
 * for the current key returns null from `loadProducerKey` and a
 * `revoked: true` result from `verifyMemento`.
 *
 * Key mementos use the simpler `Memento` shape from `mementoStore.ts`,
 * not the full `ClaimEnvelope` wrapper. The spec is explicit about not
 * bridging these in v1: key mementos are the bootstrap layer; the
 * envelope wrapper sits on top of them. Stage producers sign their
 * proof envelopes via `signMemento` here.
 */

import {
  KeyObject,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
} from "node:crypto";
import type { Db } from "../db/index.js";
import {
  hashCanonical,
  writeMemento,
  findMementoByBindingHash,
} from "../fix/runtime/mementoStore.js";
import {
  signEnvelope,
  verifyEnvelopeSignature,
} from "../claimEnvelope/sign.js";
import type { ClaimEnvelope } from "../claimEnvelope/types.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ProducerKeypair {
  publicKey: KeyObject;
  privateKey: KeyObject;
  /**
   * CID of the key memento that published this public key. Empty until
   * `publishPublicKey` runs. The CID also identifies the keypair in
   * rotation/revocation mementos.
   */
  publicKeyCid: string;
}

/**
 * Input shape for `signMemento`. Mirrors what `signEnvelope` accepts —
 * a claim envelope without `cid`, with optional unset `producerSignature`.
 */
export type SignableEnvelope = Omit<ClaimEnvelope, "cid"> & {
  cid?: string;
};

export interface VerifyResult {
  valid: boolean;
  reason?: string;
  /** CID of the key memento whose public key verified the signature. */
  keyCid?: string;
  /** 0 = original key; 1 = first rotation; 2 = second rotation; ... */
  keyRotationDepth?: number;
  /**
   * True if the producer's current key has been revoked. Implies
   * `valid: false`. Mementos signed BEFORE the revocation are still
   * cryptographically valid; consumer policy decides whether to trust
   * them. v1 reports revoked=true for any signature against a producer
   * whose current key is revoked.
   */
  revoked?: boolean;
}

const KEY_PRODUCER = "provekit-key-mgmt@v1";
const KIND_PUBLIC = "producer-public-key";
const KIND_ROTATION = "producer-key-rotation";
const KIND_REVOKE = "producer-key-revoke";

// ---------------------------------------------------------------------------
// Keypair generation
// ---------------------------------------------------------------------------

/**
 * Standard pkcs8 prefix for an ed25519 private key carrying a 32-byte
 * raw seed. RFC 8410. Concatenating this with the seed produces a
 * 48-byte DER blob that `createPrivateKey({format:"der", type:"pkcs8"})`
 * accepts deterministically: same seed in, same key bytes out.
 *
 * (The spec's suggested `format:"raw"` path is not supported by Node's
 * `createPrivateKey` API; pkcs8 with the 16-byte algorithm-id prefix
 * is the equivalent deterministic path.)
 */
const ED25519_PKCS8_PREFIX = Buffer.from(
  "302e020100300506032b657004220420",
  "hex",
);

/**
 * Generate an ed25519 keypair.
 *
 * For tests, pass `options.seed` (exactly 32 bytes) to get a
 * deterministic keypair. Without a seed, a cryptographically random
 * keypair is generated via `generateKeyPairSync("ed25519")`.
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
    return { publicKey, privateKey, publicKeyCid: "" };
  }
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  return { publicKey, privateKey, publicKeyCid: "" };
}

// ---------------------------------------------------------------------------
// Memento shape helpers
// ---------------------------------------------------------------------------

function publicKeyToBase64(key: KeyObject): string {
  return key.export({ format: "der", type: "spki" }).toString("base64");
}

function publicKeyFromBase64(b64: string): KeyObject {
  return createPublicKey({
    key: Buffer.from(b64, "base64"),
    format: "der",
    type: "spki",
  });
}

function publicKeyBindingHash(producerId: string): string {
  return hashCanonical({ producerId, kind: KIND_PUBLIC });
}

function publicKeyPropertyHash(publicKeyDer: string): string {
  return hashCanonical({ publicKeyDer });
}

function rotationBindingHash(producerId: string, oldKeyCid: string): string {
  return hashCanonical({ producerId, kind: KIND_ROTATION, oldKeyCid });
}

function rotationPropertyHash(newPublicKeyDer: string): string {
  return hashCanonical({ newPublicKeyDer });
}

function revokeBindingHash(
  producerId: string,
  compromisedKeyCid: string,
): string {
  return hashCanonical({
    producerId,
    kind: KIND_REVOKE,
    compromisedKeyCid,
  });
}

function revokePropertyHash(reason: string): string {
  return hashCanonical({ reason });
}

// ---------------------------------------------------------------------------
// publishPublicKey
// ---------------------------------------------------------------------------

/**
 * Publish a producer's public key as a content-addressed memento.
 *
 * Memento shape:
 *   bindingHash  = hashCanonical({producerId, kind: "producer-public-key"})
 *   propertyHash = hashCanonical({publicKeyDer: <base64 DER>})
 *   verdict      = "holds"
 *   witness      = JSON {kind, producerId, publicKeyDer}
 *   producedBy   = "provekit-key-mgmt@v1"
 *
 * Mutates `keypair.publicKeyCid` to the CID of the written memento and
 * returns the same CID.
 *
 * v1 assumes one original public-key memento per producerId. Re-publishing
 * over the same producerId is undefined (use `rotateKey` for upgrades).
 */
export function publishPublicKey(
  producerId: string,
  keypair: ProducerKeypair,
  db: Db,
): string {
  const publicKeyDer = publicKeyToBase64(keypair.publicKey);
  const witness = JSON.stringify({
    kind: KIND_PUBLIC,
    producerId,
    publicKeyDer,
  });
  const row = writeMemento(db, {
    bindingHash: publicKeyBindingHash(producerId),
    propertyHash: publicKeyPropertyHash(publicKeyDer),
    verdict: "holds",
    witness,
    producedBy: KEY_PRODUCER,
  });
  const cid = row.cid!;
  keypair.publicKeyCid = cid;
  return cid;
}

// ---------------------------------------------------------------------------
// rotateKey
// ---------------------------------------------------------------------------

/**
 * Publish a key-rotation memento. The new key replaces the old.
 *
 * Consumers walk the chain via `loadProducerKey` and arrive at the new
 * key. The rotation memento's `inputCids` refers to the old key memento,
 * making the rotation chain a Merkle DAG.
 *
 * Caller responsibility: the new keypair should already have been
 * generated. This call DOES NOT publish the new key as its own
 * `producer-public-key` memento — the rotation memento itself carries
 * the new key bytes. The `newKeypair.publicKeyCid` is set to the
 * rotation memento's CID so subsequent rotations chain correctly.
 */
export function rotateKey(
  producerId: string,
  oldKeypair: ProducerKeypair,
  newKeypair: ProducerKeypair,
  db: Db,
): string {
  if (!oldKeypair.publicKeyCid) {
    throw new Error(
      "old keypair has no publicKeyCid; publishPublicKey must run first",
    );
  }
  const newPublicKeyDer = publicKeyToBase64(newKeypair.publicKey);
  const witness = JSON.stringify({
    kind: KIND_ROTATION,
    producerId,
    oldKeyCid: oldKeypair.publicKeyCid,
    newPublicKeyDer,
  });
  const row = writeMemento(db, {
    bindingHash: rotationBindingHash(producerId, oldKeypair.publicKeyCid),
    propertyHash: rotationPropertyHash(newPublicKeyDer),
    verdict: "holds",
    witness,
    producedBy: KEY_PRODUCER,
    inputCids: [oldKeypair.publicKeyCid],
  });
  const cid = row.cid!;
  newKeypair.publicKeyCid = cid;
  return cid;
}

// ---------------------------------------------------------------------------
// revokeKey
// ---------------------------------------------------------------------------

/**
 * Publish a revocation memento for a compromised key.
 *
 * After this call, `loadProducerKey` returns null and `verifyMemento`
 * returns `{valid: false, revoked: true}` for any memento whose signature
 * resolves to the revoked key via the rotation chain.
 *
 * `signingKeypair` exists for forward-compatibility: in a multi-sig or
 * out-of-band rotation scheme, the revoke memento itself would be signed
 * by an authority key. v1 accepts the parameter but does not sign the
 * memento; the revoke verdict is held by the memento store's integrity.
 * Reserved for v2 to attach a signature without an API change.
 */
export function revokeKey(
  producerId: string,
  compromisedKeypair: ProducerKeypair,
  signingKeypair: ProducerKeypair,
  db: Db,
  reason?: string,
): string {
  void signingKeypair; // reserved for v2 signed-revoke
  if (!compromisedKeypair.publicKeyCid) {
    throw new Error(
      "compromised keypair has no publicKeyCid; cannot revoke an unpublished key",
    );
  }
  const reasonText = reason ?? "";
  const witness = JSON.stringify({
    kind: KIND_REVOKE,
    producerId,
    compromisedKeyCid: compromisedKeypair.publicKeyCid,
    reason: reasonText,
  });
  const row = writeMemento(db, {
    bindingHash: revokeBindingHash(producerId, compromisedKeypair.publicKeyCid),
    propertyHash: revokePropertyHash(reasonText),
    verdict: "violated",
    witness,
    producedBy: KEY_PRODUCER,
    inputCids: [compromisedKeypair.publicKeyCid],
  });
  return row.cid!;
}

// ---------------------------------------------------------------------------
// Rotation chain walker
// ---------------------------------------------------------------------------

interface ResolvedKey {
  publicKey: KeyObject;
  keyCid: string;
  rotationDepth: number;
  revoked: boolean;
}

/**
 * Internal walker. Resolves the current public key for a producerId,
 * tracks rotation depth, and detects revocation.
 *
 * Algorithm:
 *   1. Find the original public-key memento for producerId.
 *   2. Follow rotation mementos (bindingHash encodes the previous CID)
 *      until no rotation exists for the current CID.
 *   3. Check for a revoke memento referencing the final CID.
 *
 * Cycles in the rotation chain are not expected (CIDs are
 * content-addressed) but a `seen` set guards against corruption.
 */
function resolveProducerKey(
  producerId: string,
  db: Db,
): ResolvedKey | { error: "no-key" } {
  const originals = findMementoByBindingHash(
    db,
    publicKeyBindingHash(producerId),
    { producedBy: KEY_PRODUCER },
  );
  if (originals.length === 0) {
    return { error: "no-key" };
  }
  // v1 assumes one original per producerId. If multiple exist, take the
  // first deterministically; downstream policy is undefined.
  const original = originals[0];
  let currentCid = original.cid!;
  let currentDer = (JSON.parse(original.witness!) as { publicKeyDer: string })
    .publicKeyDer;
  let depth = 0;
  const seen = new Set<string>([currentCid]);

  for (;;) {
    const rotations = findMementoByBindingHash(
      db,
      rotationBindingHash(producerId, currentCid),
      { producedBy: KEY_PRODUCER },
    );
    if (rotations.length === 0) break;
    // v1 assumes one rotation per (producerId, oldKeyCid); take the first.
    const rotation = rotations[0];
    const nextCid = rotation.cid!;
    if (seen.has(nextCid)) {
      throw new Error(
        `rotation chain cycle detected for producer "${producerId}" at ${nextCid}`,
      );
    }
    seen.add(nextCid);
    const parsed = JSON.parse(rotation.witness!) as {
      newPublicKeyDer: string;
    };
    currentDer = parsed.newPublicKeyDer;
    currentCid = nextCid;
    depth += 1;
  }

  const revokes = findMementoByBindingHash(
    db,
    revokeBindingHash(producerId, currentCid),
    { producedBy: KEY_PRODUCER },
  );
  const revoked = revokes.length > 0;

  return {
    publicKey: publicKeyFromBase64(currentDer),
    keyCid: currentCid,
    rotationDepth: depth,
    revoked,
  };
}

// ---------------------------------------------------------------------------
// loadProducerKey (public)
// ---------------------------------------------------------------------------

/**
 * Resolve the current valid public key for a producerId.
 *
 * Walks the rotation chain to its tip. Returns null if the tip key has
 * been revoked. Throws if the producer has never published a public
 * key (no bootstrap exists).
 */
export function loadProducerKey(
  producerId: string,
  db: Db,
): KeyObject | null {
  const result = resolveProducerKey(producerId, db);
  if ("error" in result) {
    throw new Error(`no public key found for producer "${producerId}"`);
  }
  return result.revoked ? null : result.publicKey;
}

// ---------------------------------------------------------------------------
// signMemento / verifyMemento
// ---------------------------------------------------------------------------

/**
 * Sign a claim envelope using the given private key.
 *
 * Thin wrapper over `signEnvelope` from src/claimEnvelope/sign.ts. The
 * envelope's `cid` and `producerSignature` are elided from the signing
 * input (already handled by `signEnvelope`).
 */
export function signMemento(
  envelope: SignableEnvelope,
  privateKey: KeyObject,
): string {
  return signEnvelope(envelope, privateKey);
}

/**
 * Verify a claim envelope's signature.
 *
 * The producer's identity is taken from `envelope.producedBy`. The
 * rotation chain for that producerId is walked and the signature is
 * checked against the resolved tip key. v1 uses `producedBy` as the
 * producerId verbatim — i.e. keys are version-scoped (a v4.14 producer
 * uses a different key from v4.13). This makes producer-version upgrade
 * an explicit `rotateKey` event. Document caveat: callers wishing
 * version-independent keys can publish under the bare name and route
 * `verifyMemento` callers through a producerId-mapping shim — but v1
 * does not provide that shim.
 */
export function verifyMemento(
  envelope: ClaimEnvelope,
  db: Db,
): VerifyResult {
  if (!envelope.producerSignature) {
    return { valid: false, reason: "envelope has no producerSignature" };
  }
  const resolved = resolveProducerKey(envelope.producedBy, db);
  if ("error" in resolved) {
    return {
      valid: false,
      reason: `no public key found for producer "${envelope.producedBy}"`,
    };
  }
  if (resolved.revoked) {
    return {
      valid: false,
      revoked: true,
      reason: `producer "${envelope.producedBy}" key is revoked`,
      keyCid: resolved.keyCid,
      keyRotationDepth: resolved.rotationDepth,
    };
  }
  const ok = verifyEnvelopeSignature(envelope, resolved.publicKey);
  if (!ok) {
    return {
      valid: false,
      reason: "ed25519 signature verification failed",
      keyCid: resolved.keyCid,
      keyRotationDepth: resolved.rotationDepth,
    };
  }
  return {
    valid: true,
    keyCid: resolved.keyCid,
    keyRotationDepth: resolved.rotationDepth,
  };
}

