/**
 * Universal claim envelope — main entrypoint.
 *
 * Spec: protocol/specs/2026-04-30-memento-envelope-grammar.md
 *
 * Re-exports all public types and functions for consumers.
 */

// Types
export type {
  Verdict,
  EvidenceVariant,
  Z3ModelEvidence,
  Z3UnsatEvidence,
  PatternMatchEvidence,
  TypeCheckPassEvidence,
  LintPassEvidence,
  TestPassEvidence,
  TestFailEvidence,
  LlmProposalEvidence,
  MutationWitnessEvidence,
  WorkflowRunEvidence,
  BridgeEvidence,
  ContractEvidence,
  ContractAuthoring,
  KitAuthorAuthoring,
  LiftAuthoring,
  LlmAuthoring,
  ImplicationEvidence,
  ExtensionDeclarationEvidence,
  ClaimEnvelope,
} from "./types.js";

export { VERDICTS } from "./types.js";

// Canonicalization
export { canonicalEncode, canonicalJsonString } from "./canonicalize.js";

// CID construction
export { computeEnvelopeCid, envelopeForHashing } from "./cid.js";

// Signing
export { signEnvelope, verifyEnvelopeSignature } from "./sign.js";

// Minting helpers
export {
  mintMemento,
  mintBridge,
  mintContract,
  mintImplication,
  mintExtensionDeclaration,
  mintAndVerifyMemento,
} from "./mint.js";
export type {
  MintArgs,
  MintBridgeArgs,
  MintContractArgs,
  MintImplicationArgs,
  MintExtensionDeclarationArgs,
} from "./mint.js";

// Validation
export type { KeyResolver, ValidationResult } from "./validate.js";
export { validateEnvelope } from "./validate.js";

// Variants registry
export { VARIANT_SCHEMA_CIDS } from "./variants/index.js";
export type { VariantKind } from "./variants/index.js";
