/**
 * Real Ed25519 signing/verification tests for extension and bridge
 * declarations. Uses node:crypto's standard Ed25519 (RFC 8032). A
 * declaration signed in TS verifies in any kit that conforms to the
 * signatures spec; this test exercises only the TS path but the
 * canonical bytes + signature payload are identical across kits.
 */

import { describe, it, expect } from "vitest";
import { generateKeyPairSync } from "node:crypto";
import {
  signExtensionDeclaration,
  signBridgeDeclaration,
  verifyExtensionSignature,
  verifyBridgeSignature,
  canonicalBytesForExtension,
} from "./signing.js";
import type { ExtensionDeclaration } from "./registry.js";
import type { PrimitiveBridgeDeclaration } from "./bridges.js";

function makeKeypair() {
  return generateKeyPairSync("ed25519");
}

function makeSortDecl(): ExtensionDeclaration {
  return {
    introduces: "sort",
    name: "FixedPoint8",
    semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
    compilers: ["smt-lib"],
  };
}

function makeBridgeDecl(): PrimitiveBridgeDeclaration {
  return {
    irName: "parseInt",
    irArgSorts: ["String"],
    irReturnSort: "Int",
    sourceLayer: "ts-kit",
    targetContractCid: "bafy_V8_PARSEINT",
    targetLayer: "v8",
  };
}

describe("Ed25519 extension declaration signing", () => {
  it("signs and verifies a sort declaration round-trip", () => {
    const { privateKey, publicKey } = makeKeypair();
    const decl = makeSortDecl();
    const signed = signExtensionDeclaration(decl, privateKey);
    expect(signed.signature).toBeTruthy();
    expect(verifyExtensionSignature(signed, publicKey)).toBe(true);
  });

  it("rejects a declaration tampered with after signing", () => {
    const { privateKey, publicKey } = makeKeypair();
    const decl = makeSortDecl();
    const signed = signExtensionDeclaration(decl, privateKey);
    const tampered = { ...signed, name: "Tampered8" };
    expect(verifyExtensionSignature(tampered, publicKey)).toBe(false);
  });

  it("rejects a declaration verified against a different public key", () => {
    const { privateKey } = makeKeypair();
    const otherPair = makeKeypair();
    const decl = makeSortDecl();
    const signed = signExtensionDeclaration(decl, privateKey);
    expect(verifyExtensionSignature(signed, otherPair.publicKey)).toBe(false);
  });

  it("returns false on unsigned declaration (fail closed)", () => {
    const { publicKey } = makeKeypair();
    const decl = makeSortDecl();
    expect(verifyExtensionSignature(decl, publicKey)).toBe(false);
  });

  it("canonical bytes are stable under key reordering", () => {
    const reordered: ExtensionDeclaration = {
      compilers: ["smt-lib"],
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      name: "FixedPoint8",
      introduces: "sort",
    };
    const original = makeSortDecl();
    expect(canonicalBytesForExtension(reordered)).toEqual(canonicalBytesForExtension(original));
  });
});

describe("Ed25519 bridge declaration signing", () => {
  it("signs and verifies a bridge declaration round-trip", () => {
    const { privateKey, publicKey } = makeKeypair();
    const decl = makeBridgeDecl();
    const signed = signBridgeDeclaration(decl, privateKey);
    expect(signed.signature).toBeTruthy();
    expect(verifyBridgeSignature(signed, publicKey)).toBe(true);
  });

  it("rejects a tampered bridge", () => {
    const { privateKey, publicKey } = makeKeypair();
    const decl = makeBridgeDecl();
    const signed = signBridgeDeclaration(decl, privateKey);
    const tampered = { ...signed, targetContractCid: "bafy_FAKE" };
    expect(verifyBridgeSignature(tampered, publicKey)).toBe(false);
  });

  it("rejects an unsigned bridge", () => {
    const { publicKey } = makeKeypair();
    const decl = makeBridgeDecl();
    expect(verifyBridgeSignature(decl, publicKey)).toBe(false);
  });
});
