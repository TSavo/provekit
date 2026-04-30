/**
 * Standard evidence variant types and schema CIDs.
 *
 * Schema CIDs here are placeholder hex32 values. In production these
 * are computed at package-publish time by content-hashing the schema
 * definition files in `evidence-schemas/`. Producers bake the CIDs
 * from this registry into their variant emitters.
 *
 * Spec: docs/specs/2026-04-29-universal-claim-envelope.md §Evidence variants
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
  LegacyWitnessEvidence,
  BridgeEvidence,
  EvidenceVariant,
} from "../types.js";

/**
 * Registry of standard variant kind → schema CID mappings.
 *
 * Schema CIDs are stable content-addresses for the schema definition.
 * These are the canonical values producers MUST use for v1 variants.
 * Replace with real CIDs computed from evidence-schemas/ at publish time.
 */
export const VARIANT_SCHEMA_CIDS: Readonly<Record<string, string>> = {
  "z3-model":         "00000000000000010000000000000001",
  "z3-unsat":         "00000000000000020000000000000002",
  "pattern-match":    "00000000000000030000000000000003",
  "type-check-pass":  "00000000000000040000000000000004",
  "lint-pass":        "00000000000000050000000000000005",
  "test-pass":        "00000000000000060000000000000006",
  "test-fail":        "00000000000000070000000000000007",
  "llm-proposal":     "00000000000000080000000000000008",
  "mutation-witness": "00000000000000090000000000000009",
  "workflow-run":     "0000000000000000a0000000000000a0",
  "legacy-witness":   "0000000000000000b0000000000000b0",
  "bridge":           "0000000000000000c0000000000000c0",
} as const;

export type VariantKind = keyof typeof VARIANT_SCHEMA_CIDS;
