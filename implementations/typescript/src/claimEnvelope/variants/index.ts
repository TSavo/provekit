/**
 * Standard evidence variant types and schema CIDs.
 *
 * Schema CIDs here are placeholder self-identifying hashes
 * ("blake3-512:" + 128 hex chars) per protocol v1.1.0. They are valid
 * regex-wise but the bytes do NOT yet resolve to real schema mementos;
 * a follow-up cut substitutes the real schema-memento CIDs computed by
 * content-hashing the schema definition files. Producers bake the CIDs
 * from this registry into their variant emitters.
 *
 * Spec: protocol/specs/2026-04-29-universal-claim-envelope.md §Evidence variants
 *       protocol/specs/2026-04-30-memento-envelope-grammar.md §Self-identifying
 */

// Re-export variant types from the main types file for convenience.
export type {
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
  EvidenceVariant,
} from "../types.js";

/**
 * Registry of standard variant kind → schema CID mappings. Locked to
 * match the cross-language reference implementations under v1.1: the
 * `contract` variant inherits the previous `property` slot's CID
 * (the wire shape of the body changed; the schema slot stays).
 */
/** Helper to build a placeholder self-identifying schema CID. */
function placeholderCid(suffix: string): string {
  // 128 lowercase hex chars total: zero-pad on the left, encode the
  // distinguishing suffix on the right so each variant has a unique
  // (but obviously placeholder) CID. The shape passes the v1.1.0
  // regex `^[a-z0-9]+-[0-9]+:[0-9a-f]+$`.
  const padded = suffix.padStart(128, "0");
  return `blake3-512:${padded}`;
}

export const VARIANT_SCHEMA_CIDS: Readonly<Record<string, string>> = {
  "z3-model":               placeholderCid("01"),
  "z3-unsat":               placeholderCid("02"),
  "pattern-match":          placeholderCid("03"),
  "type-check-pass":        placeholderCid("04"),
  "lint-pass":              placeholderCid("05"),
  "test-pass":              placeholderCid("06"),
  "test-fail":              placeholderCid("07"),
  "llm-proposal":           placeholderCid("08"),
  "mutation-witness":       placeholderCid("09"),
  "workflow-run":           placeholderCid("a0"),
  "bridge":                 placeholderCid("c0"),
  "contract":               placeholderCid("d0"),
  "implication":            placeholderCid("d8"),
  "extension-declaration":  placeholderCid("e0"),
} as const;

export type VariantKind = keyof typeof VARIANT_SCHEMA_CIDS;
