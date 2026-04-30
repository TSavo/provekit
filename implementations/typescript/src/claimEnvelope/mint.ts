/**
 * Memento minting utilities — high-level helpers for producers.
 *
 * Lower-level primitives are at:
 *   - sign.ts (signEnvelope, verifyEnvelopeSignature)
 *   - cid.ts (computeEnvelopeCid)
 *
 * These mint helpers compose the lower-level primitives into "common
 * memento shapes" so producers don't have to wire signEnvelope +
 * computeEnvelopeCid + envelope construction manually each time.
 *
 * In keeping with the framework's scope discipline (see
 * protocol/specs/2026-04-29-correctness-is-a-hash.md §"What ProvekIt is"),
 * these helpers MINT mementos. They do NOT walk DAGs, traverse
 * bridges, or audit chains. Walking is downstream tooling.
 */

import type { KeyObject } from "node:crypto";
import { signEnvelope, verifyEnvelopeSignature } from "./sign.js";
import { computeEnvelopeCid } from "./cid.js";
import type {
  ClaimEnvelope,
  Verdict,
  EvidenceVariant,
  BridgeEvidence,
  LegacyWitnessEvidence,
  PropertyEvidence,
} from "./types.js";
import type { IrFormula, BindingScope } from "../ir/formulas.js";
import { VARIANT_SCHEMA_CIDS } from "./variants/index.js";

// ---------------------------------------------------------------------------
// Common minting interface
// ---------------------------------------------------------------------------

export interface MintArgs {
  bindingHash: string;
  propertyHash: string;
  verdict: Verdict;
  producedBy: string;
  producedAt?: string;          // defaults to new Date().toISOString()
  inputCids?: string[];         // defaults to []
  evidence: EvidenceVariant;
  privateKey: KeyObject | Buffer | string;
}

/**
 * Mint a signed claim envelope.
 *
 * 1. Builds the envelope from args.
 * 2. Computes the producer signature over the canonical form.
 * 3. Computes the CID over the canonical form.
 * 4. Returns the signed, content-addressed envelope.
 *
 * The caller is responsible for verifying the envelope's signature
 * (via verifyEnvelopeSignature) if they want round-trip confirmation.
 */
export function mintMemento(args: MintArgs): ClaimEnvelope {
  const unsigned = {
    schemaVersion: "1" as const,
    bindingHash: args.bindingHash,
    propertyHash: args.propertyHash,
    verdict: args.verdict,
    producedBy: args.producedBy,
    producedAt: args.producedAt ?? new Date().toISOString(),
    inputCids: [...(args.inputCids ?? [])].sort(),
    evidence: args.evidence,
  };
  const signature = signEnvelope(unsigned, args.privateKey);
  const cid = computeEnvelopeCid(unsigned);
  return { ...unsigned, producerSignature: signature, cid };
}

// ---------------------------------------------------------------------------
// Bridge memento helper
// ---------------------------------------------------------------------------

export interface MintBridgeArgs {
  bindingHash: string;
  propertyHash: string;
  producedBy: string;
  producedAt?: string;
  privateKey: KeyObject | Buffer | string;
  sourceSymbol: string;
  sourceLayer: string;
  targetContractCid: string;
  targetLayer: string;
  /** IR argument sorts of the bridged primitive (SortRef[]). Required. */
  irArgSorts: unknown[];
  /** IR return sort of the bridged primitive (SortRef). Required. */
  irReturnSort: unknown;
  notes?: string;
}

/**
 * Mint a bridge memento — a content-addressed edge declaring that a
 * host-language symbol bridges to a deeper-layer published contract.
 *
 * The bridge composes by hash: `inputCids: [targetContractCid]`. Walking
 * the bridge means traversing to the deeper layer (which is a different
 * codebase, possibly a different language). That walking work is the
 * AUDITOR's job, not the framework's. ProvekIt mints the bridge; the
 * bridge's verdict is "I am the surface of that hash"; consumers
 * compose against the hash and stop there.
 *
 * See protocol/specs/2026-04-29-correctness-is-a-hash.md §"What ProvekIt is"
 * for the scope discipline this helper preserves.
 */
export function mintBridge(args: MintBridgeArgs): ClaimEnvelope {
  const evidence: BridgeEvidence = {
    kind: "bridge",
    schema: VARIANT_SCHEMA_CIDS["bridge"]!,
    body: {
      sourceSymbol: args.sourceSymbol,
      sourceLayer: args.sourceLayer,
      targetContractCid: args.targetContractCid,
      targetLayer: args.targetLayer,
      irArgSorts: args.irArgSorts,
      irReturnSort: args.irReturnSort,
      ...(args.notes !== undefined ? { notes: args.notes } : {}),
    },
  };
  return mintMemento({
    bindingHash: args.bindingHash,
    propertyHash: args.propertyHash,
    verdict: "holds",
    producedBy: args.producedBy,
    ...(args.producedAt !== undefined ? { producedAt: args.producedAt } : {}),
    inputCids: [args.targetContractCid],
    evidence,
    privateKey: args.privateKey,
  });
}

// ---------------------------------------------------------------------------
// Legacy-witness memento helper
// ---------------------------------------------------------------------------

export interface MintLegacyWitnessArgs {
  bindingHash: string;
  propertyHash: string;
  verdict?: Verdict;
  producedBy: string;
  producedAt?: string;
  inputCids?: string[];
  privateKey: KeyObject | Buffer | string;
  rawWitness: string;
}

// ---------------------------------------------------------------------------
// Property memento helper
// ---------------------------------------------------------------------------

export interface MintPropertyArgs {
  bindingHash: string;
  propertyHash: string;
  verdict?: Verdict;
  producedBy: string;
  producedAt?: string;
  inputCids?: string[];
  privateKey: KeyObject | Buffer | string;
  /** The IrFormula stating the property. Embedded directly. */
  irFormula: IrFormula;
  /** The binding scope the property attaches to. */
  scope: BindingScope;
  /** IR-kit version that produced the formula (e.g., "ts-kit@1.0"). */
  irKitVersion: string;
}

/**
 * Mint a property memento — a content-addressed assertion that an IR
 * formula holds in a named binding scope. The formula is the
 * load-bearing artifact bridges point at; resolving a bridge's
 * targetContractCid yields a property memento, and its body.irFormula
 * is the precondition (or postcondition, or invariant) the verifier
 * uses to discharge call-site obligations.
 */
export function mintProperty(args: MintPropertyArgs): ClaimEnvelope {
  const evidence: PropertyEvidence = {
    kind: "property",
    schema: VARIANT_SCHEMA_CIDS["property"]!,
    body: {
      irFormula: args.irFormula,
      scope: args.scope,
      irKitVersion: args.irKitVersion,
    },
  };
  return mintMemento({
    bindingHash: args.bindingHash,
    propertyHash: args.propertyHash,
    verdict: args.verdict ?? "holds",
    producedBy: args.producedBy,
    ...(args.producedAt !== undefined ? { producedAt: args.producedAt } : {}),
    inputCids: args.inputCids ?? [],
    evidence,
    privateKey: args.privateKey,
  });
}

/**
 * Mint a legacy-witness memento — wraps an opaque producer-private
 * witness (typically the result of a Stage producer's serializeOutput)
 * in the universal envelope.
 */
export function mintLegacyWitness(args: MintLegacyWitnessArgs): ClaimEnvelope {
  const evidence: LegacyWitnessEvidence = {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
    body: {
      rawWitness: args.rawWitness,
      legacyProducerId: args.producedBy,
    },
  };
  return mintMemento({
    bindingHash: args.bindingHash,
    propertyHash: args.propertyHash,
    verdict: args.verdict ?? "holds",
    producedBy: args.producedBy,
    ...(args.producedAt !== undefined ? { producedAt: args.producedAt } : {}),
    inputCids: args.inputCids ?? [],
    evidence,
    privateKey: args.privateKey,
  });
}

// ---------------------------------------------------------------------------
// Round-trip verification helper
// ---------------------------------------------------------------------------

/**
 * Mint a memento and verify its signature round-trips.
 *
 * Convenience wrapper for tests and demos that want to assert
 * "signature is valid" before proceeding.
 *
 * Throws if signature verification fails.
 */
export function mintAndVerifyMemento(
  args: MintArgs,
  publicKey: KeyObject | Buffer | string,
): ClaimEnvelope {
  const memento = mintMemento(args);
  if (!verifyEnvelopeSignature(memento, publicKey)) {
    throw new Error(
      `signature verification failed for binding ${memento.bindingHash}`,
    );
  }
  return memento;
}
