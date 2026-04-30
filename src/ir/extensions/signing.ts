/**
 * Real Ed25519 signing + verification for extension declarations
 * and primitive bridges.
 *
 * Per the signatures-and-non-repudiation spec
 * (protocol/specs/2026-04-30-signatures-and-non-repudiation.md):
 * extension declarations and bridge declarations are signed with
 * Ed25519. The signature payload is the canonical bytes of the
 * declaration with the signature field removed; verifiers
 * recompute the canonical bytes and check the signature against
 * the declarer's public key.
 *
 * This module wraps node:crypto's Ed25519 primitives. The Rust /
 * Go / C++ kits use their language's standard Ed25519 (sodiumoxide,
 * golang.org/x/crypto/ed25519, OpenSSL); the protocol's signature
 * payload + algorithm is identical, so a TS-signed declaration
 * verifies in any kit and vice versa.
 *
 * The kit's bridge declarations are typically UNSIGNED at kit
 * authoring time (the kit doesn't sign as the deeper-layer
 * authority — V8's release team signs V8's parseInt declaration,
 * not the TS kit). The kit auto-registers bridges with empty
 * signer/signature; the verifier expects the deeper-layer's
 * SIGNED catalog to be in scope and matches via the bridge's
 * targetContractCid.
 */

import {
  createPrivateKey,
  createPublicKey,
  sign,
  verify,
  type KeyObject,
} from "node:crypto";
import type {
  ExtensionDeclaration,
  SortExtensionDeclaration,
  PredicateExtensionDeclaration,
  CtorExtensionDeclaration,
} from "./registry.js";
import type { PrimitiveBridgeDeclaration } from "./bridges.js";

// ---------------------------------------------------------------------------
// Canonicalization for signing
// ---------------------------------------------------------------------------

/**
 * Compute the canonical signing payload for an extension declaration.
 * Strips signature field and stable-orders keys so two implementations
 * produce byte-identical bytes.
 */
export function canonicalBytesForExtension(decl: ExtensionDeclaration): Uint8Array {
  // Strip any embedded signature; the signature payload is everything else.
  const { signature: _sig, ...rest } = decl as ExtensionDeclaration & { signature?: string };
  void _sig;
  const json = stableStringify(rest);
  return new TextEncoder().encode(json);
}

export function canonicalBytesForBridge(decl: PrimitiveBridgeDeclaration): Uint8Array {
  // Strip any embedded signature; the signature payload is everything else.
  const { signature: _sig, ...rest } = decl as PrimitiveBridgeDeclaration & { signature?: string };
  void _sig;
  const json = stableStringify(rest);
  return new TextEncoder().encode(json);
}

/**
 * Stable-key-order JSON serialization. Same canonicalization rule the
 * memento envelope grammar uses — sorted keys, no whitespace, JCS-shaped.
 */
function stableStringify(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) {
    return "[" + value.map(stableStringify).join(",") + "]";
  }
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, v]) => v !== undefined)
    .sort(([a], [b]) => a.localeCompare(b));
  return "{" + entries.map(([k, v]) => JSON.stringify(k) + ":" + stableStringify(v)).join(",") + "}";
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

/**
 * Sign an extension declaration with an Ed25519 private key. Returns
 * the same declaration with `signature` field populated (multibase-style
 * hex-encoded for ergonomics; production may switch to base64 or
 * multibase per the signatures spec).
 */
export function signExtensionDeclaration(
  decl: ExtensionDeclaration,
  privateKey: KeyObject | Buffer | string,
): ExtensionDeclaration & { signature: string } {
  const key = toPrivateKey(privateKey);
  const bytes = canonicalBytesForExtension(decl);
  const sig = sign(null, Buffer.from(bytes), key);
  return { ...decl, signature: sig.toString("hex") };
}

export function signBridgeDeclaration(
  decl: PrimitiveBridgeDeclaration,
  privateKey: KeyObject | Buffer | string,
): PrimitiveBridgeDeclaration & { signature: string } {
  const key = toPrivateKey(privateKey);
  const bytes = canonicalBytesForBridge(decl);
  const sig = sign(null, Buffer.from(bytes), key);
  return { ...decl, signature: sig.toString("hex") };
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/**
 * Verify an extension declaration's signature. Returns true iff
 * `signature` is a valid Ed25519 signature over the canonical bytes
 * by the holder of `publicKey`.
 *
 * Returns FALSE on missing signature (fail-closed) per the signatures
 * spec — verifiers MUST reject unsigned declarations where signatures
 * are required.
 */
export function verifyExtensionSignature(
  decl: ExtensionDeclaration & { signature?: string },
  publicKey: KeyObject | Buffer | string,
): boolean {
  if (!decl.signature) return false;
  const key = toPublicKey(publicKey);
  const bytes = canonicalBytesForExtension(decl);
  const sigBytes = Buffer.from(decl.signature, "hex");
  return verify(null, Buffer.from(bytes), key, sigBytes);
}

export function verifyBridgeSignature(
  decl: PrimitiveBridgeDeclaration & { signature?: string },
  publicKey: KeyObject | Buffer | string,
): boolean {
  if (!decl.signature) return false;
  const key = toPublicKey(publicKey);
  const bytes = canonicalBytesForBridge(decl);
  const sigBytes = Buffer.from(decl.signature, "hex");
  return verify(null, Buffer.from(bytes), key, sigBytes);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function toPrivateKey(key: KeyObject | Buffer | string): KeyObject {
  if (typeof key === "string") return createPrivateKey(key);
  if (key instanceof Buffer) return createPrivateKey(key);
  return key;
}

function toPublicKey(key: KeyObject | Buffer | string): KeyObject {
  if (typeof key === "string") return createPublicKey(key);
  if (key instanceof Buffer) return createPublicKey(key);
  return key;
}
