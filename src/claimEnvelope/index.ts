/**
 * Universal claim envelope — main entrypoint.
 *
 * Spec: docs/specs/2026-04-29-universal-claim-envelope.md
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
  LegacyWitnessEvidence,
  BridgeEvidence,
  ClaimEnvelope,
} from "./types.js";

export { VERDICTS } from "./types.js";

// Canonicalization
export { canonicalEncode, canonicalJsonString } from "./canonicalize.js";

// CID construction
export { computeEnvelopeCid, envelopeForHashing } from "./cid.js";

// Signing
export { signEnvelope, verifyEnvelopeSignature } from "./sign.js";

// Validation
export type { KeyResolver, ValidationResult } from "./validate.js";
export { validateEnvelope } from "./validate.js";

// Variants registry
export { VARIANT_SCHEMA_CIDS } from "./variants/index.js";
export type { VariantKind } from "./variants/index.js";
