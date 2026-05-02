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
 */

import type { KeyObject } from "node:crypto";
import { signEnvelope, verifyEnvelopeSignature } from "./sign.js";
import { computeEnvelopeCid } from "./cid.js";
import { canonicalEncode } from "./canonicalize.js";
import { computeCid } from "../canonicalizer/hash.js";
import type {
  ClaimEnvelope,
  Verdict,
  EvidenceVariant,
  BridgeEvidence,
  ContractEvidence,
  ContractAuthoring,
  ImplicationEvidence,
  ExtensionDeclarationEvidence,
} from "./types.js";
import type { EvidenceTerm, IrFormula } from "../ir/formulas.js";
import type { ExtensionDeclaration } from "../ir/extensions/registry.js";
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
  producedBy: string;
  producedAt?: string;
  privateKey: KeyObject | Buffer | string;
  sourceSymbol: string;
  sourceLayer: string;
  targetContractCid: string;
  /**
   * Forward pin: the `.proof` bundle CID this bridge commits to. When
   * set, the verifier enforces BridgeDeclaration.ConsequentBundlePinned
   * (see BridgeEvidence.body.targetProofCid). Optional for back-compat;
   * the v1.4 grammar requires it for new bridges.
   */
  targetProofCid?: string;
  targetLayer: string;
  irArgSorts: unknown[];
  irReturnSort: unknown;
  notes?: string;
}

/**
 * Mint a bridge memento — a content-addressed edge declaring that a
 * host-language symbol bridges to a deeper-layer published contract.
 *
 * Wrapper hashes are DERIVED per the bridge role's spec:
 *   bindingHash  = computeCid(canonical({sourceLayer, sourceSymbol}))
 *   propertyHash = computeCid(canonical("bridge:" + sourceSymbol))
 */
export function mintBridge(args: MintBridgeArgs): ClaimEnvelope {
  const evidence: BridgeEvidence = {
    kind: "bridge",
    schema: VARIANT_SCHEMA_CIDS["bridge"]!,
    body: {
      sourceSymbol: args.sourceSymbol,
      sourceLayer: args.sourceLayer,
      targetContractCid: args.targetContractCid,
      ...(args.targetProofCid !== undefined
        ? { targetProofCid: args.targetProofCid }
        : {}),
      targetLayer: args.targetLayer,
      irArgSorts: args.irArgSorts,
      irReturnSort: args.irReturnSort,
      ...(args.notes !== undefined ? { notes: args.notes } : {}),
    },
  };
  const bindingHash = computeCid(
    canonicalEncode({ sourceLayer: args.sourceLayer, sourceSymbol: args.sourceSymbol }),
  );
  const propertyHash = computeCid(
    canonicalEncode("bridge:" + args.sourceSymbol),
  );
  return mintMemento({
    bindingHash,
    propertyHash,
    verdict: "holds",
    producedBy: args.producedBy,
    ...(args.producedAt !== undefined ? { producedAt: args.producedAt } : {}),
    inputCids: [args.targetContractCid],
    evidence,
    privateKey: args.privateKey,
  });
}

// ---------------------------------------------------------------------------
// Contract memento helper
// ---------------------------------------------------------------------------

export interface MintContractArgs {
  producedBy: string;
  producedAt?: string;
  inputCids?: string[];
  privateKey: KeyObject | Buffer | string;
  /** The contract's name (e.g., "parseInt"). */
  contractName: string;
  /** Variable name the post-formula uses to reference the return value. */
  outBinding?: string;
  /** Optional precondition formula (IR-JSON). */
  pre?: IrFormula;
  /** Optional postcondition formula (IR-JSON). */
  post?: IrFormula;
  /** Optional inductive invariant formula (IR-JSON). */
  inv?: IrFormula;
  /** Optional IR-level proof certificate (EvidenceTerm) attached at authoring time. */
  evidence?: EvidenceTerm;
  /** Authoring provenance (kit-author / lift / llm). */
  authoring: ContractAuthoring;
}

/**
 * Mint a contract memento per the v1.1 protocol cut.
 *
 * DERIVED fields (caller does NOT supply):
 *   preHash      = computeCid(canonical(pre))   when pre present
 *   postHash     = computeCid(canonical(post))  when post present
 *   invHash      = computeCid(canonical(inv))   when inv present
 *   propertyHash = computeCid(canonical({pre?, post?, inv?, outBinding}))
 *   bindingHash  = computeCid(canonical({producerId, contractName, propertyHash}))
 *
 * `pre`, `post`, `inv` are each optional; at least one MUST be present.
 * Empty slots are OMITTED from the body (not encoded as null) so the
 * canonical encoding is JCS-friendly.
 */
export function mintContract(args: MintContractArgs): ClaimEnvelope {
  if (args.pre === undefined && args.post === undefined && args.inv === undefined) {
    throw new Error(
      `mintContract("${args.contractName}"): at least one of pre/post/inv must be provided`,
    );
  }
  const outBinding = args.outBinding ?? "out";

  const body: ContractEvidence["body"] = {
    contractName: args.contractName,
    outBinding,
    authoring: args.authoring,
  };
  if (args.pre !== undefined) {
    body.pre = args.pre;
    body.preHash = computeCid(canonicalEncode(args.pre));
  }
  if (args.post !== undefined) {
    body.post = args.post;
    body.postHash = computeCid(canonicalEncode(args.post));
  }
  if (args.inv !== undefined) {
    body.inv = args.inv;
    body.invHash = computeCid(canonicalEncode(args.inv));
  }
  if (args.evidence !== undefined) {
    body.irEvidence = args.evidence;
  }

  // propertyHash hashes the semantic identity: {pre?, post?, inv?, outBinding}.
  const propertyIdentity: Record<string, unknown> = { outBinding };
  if (args.pre !== undefined) propertyIdentity.pre = args.pre;
  if (args.post !== undefined) propertyIdentity.post = args.post;
  if (args.inv !== undefined) propertyIdentity.inv = args.inv;
  const propertyHash = computeCid(canonicalEncode(propertyIdentity));

  const bindingHash = computeCid(
    canonicalEncode({
      producerId: args.producedBy,
      contractName: args.contractName,
      propertyHash,
    }),
  );

  const evidence: ContractEvidence = {
    kind: "contract",
    schema: VARIANT_SCHEMA_CIDS["contract"]!,
    body,
  };

  return mintMemento({
    bindingHash,
    propertyHash,
    verdict: "holds",
    producedBy: args.producedBy,
    ...(args.producedAt !== undefined ? { producedAt: args.producedAt } : {}),
    inputCids: args.inputCids ?? [],
    evidence,
    privateKey: args.privateKey,
  });
}

// ---------------------------------------------------------------------------
// Implication memento helper
// ---------------------------------------------------------------------------

export interface MintImplicationArgs {
  producedBy: string;
  producedAt?: string;
  privateKey: KeyObject | Buffer | string;
  antecedentHash: string;
  consequentHash: string;
  antecedentCid: string;
  consequentCid: string;
  antecedentSlot: "pre" | "post" | "inv";
  consequentSlot: "pre" | "post" | "inv";
  prover: string;
  proverRunMs: number;
  smtLibInput?: string;
  proofWitness?: string;
}

/**
 * Mint an implication memento — a signed proof witness that one IR
 * formula universally implies another. DERIVED fields:
 *   bindingHash  = computeCid(canonical({antecedentHash, consequentHash}))
 *   propertyHash = computeCid(canonical("implication:" + antecedentHash + ":" + consequentHash))
 *   inputCids    = [antecedentCid, consequentCid] lex-sorted
 */
export function mintImplication(args: MintImplicationArgs): ClaimEnvelope {
  const body: ImplicationEvidence["body"] = {
    antecedentHash: args.antecedentHash,
    consequentHash: args.consequentHash,
    antecedentCid: args.antecedentCid,
    consequentCid: args.consequentCid,
    antecedentSlot: args.antecedentSlot,
    consequentSlot: args.consequentSlot,
    prover: args.prover,
    proverRunMs: args.proverRunMs,
  };
  if (args.smtLibInput !== undefined) body.smtLibInput = args.smtLibInput;
  if (args.proofWitness !== undefined) body.proofWitness = args.proofWitness;

  const bindingHash = computeCid(
    canonicalEncode({
      antecedentHash: args.antecedentHash,
      consequentHash: args.consequentHash,
    }),
  );
  const propertyHash = computeCid(
    canonicalEncode(
      "implication:" + args.antecedentHash + ":" + args.consequentHash,
    ),
  );

  const evidence: ImplicationEvidence = {
    kind: "implication",
    schema: VARIANT_SCHEMA_CIDS["implication"]!,
    body,
  };

  return mintMemento({
    bindingHash,
    propertyHash,
    verdict: "holds",
    producedBy: args.producedBy,
    ...(args.producedAt !== undefined ? { producedAt: args.producedAt } : {}),
    inputCids: [args.antecedentCid, args.consequentCid].sort(),
    evidence,
    privateKey: args.privateKey,
  });
}

// ---------------------------------------------------------------------------
// Extension-declaration memento helper
// ---------------------------------------------------------------------------

export interface MintExtensionDeclarationArgs {
  bindingHash: string;
  propertyHash: string;
  verdict?: Verdict;
  producedBy: string;
  producedAt?: string;
  inputCids?: string[];
  privateKey: KeyObject | Buffer | string;
  /** The IR extension declaration (sort/predicate/ctor introduction). */
  declaration: ExtensionDeclaration;
}

/**
 * Mint an extension-declaration memento — a content-addressed claim
 * that introduces a new sort, predicate, or ctor into the IR
 * extension protocol.
 */
export function mintExtensionDeclaration(args: MintExtensionDeclarationArgs): ClaimEnvelope {
  const evidence: ExtensionDeclarationEvidence = {
    kind: "extension-declaration",
    schema: VARIANT_SCHEMA_CIDS["extension-declaration"]!,
    body: {
      declaration: args.declaration,
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
