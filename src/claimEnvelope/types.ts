/**
 * Universal claim envelope — TypeScript types.
 *
 * Spec: docs/specs/2026-04-29-universal-claim-envelope.md
 *
 * The wrapper schema is fixed. New evidence kinds slot into the
 * EvidenceVariant discriminated union without changing wrapper fields.
 */

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

export type Verdict = "holds" | "violated" | "decayed" | "undecidable" | "error";

export const VERDICTS: ReadonlySet<Verdict> = new Set([
  "holds",
  "violated",
  "decayed",
  "undecidable",
  "error",
]);

// ---------------------------------------------------------------------------
// Evidence variants
// ---------------------------------------------------------------------------

export interface Z3ModelEvidence {
  kind: "z3-model";
  schema: string; // hex32 CID of the variant schema definition
  body: {
    smtLibInput: string;
    z3Verdict: "sat";
    model: string;
    counterexample: Record<string, unknown>;
    z3RunMs: number;
  };
}

export interface Z3UnsatEvidence {
  kind: "z3-unsat";
  schema: string;
  body: {
    smtLibInput: string;
    z3Verdict: "unsat";
    proof?: string;
    z3RunMs: number;
  };
}

export interface PatternMatchEvidence {
  kind: "pattern-match";
  schema: string;
  body: {
    pattern: string;
    matchedNodes: string[];
    matchedCaptures: Record<string, unknown>;
  };
}

export interface TypeCheckPassEvidence {
  kind: "type-check-pass";
  schema: string;
  body: {
    checker: string;
    checkerVersion: string;
    symbol: string;
    resolvedType?: string;
    diagnosticsClean: true;
  };
}

export interface LintPassEvidence {
  kind: "lint-pass";
  schema: string;
  body: {
    linter: string;
    linterVersion: string;
    rulesetHash: string; // hex32
    warnings: 0;
  };
}

export interface TestPassEvidence {
  kind: "test-pass";
  schema: string;
  body: {
    runner: string;
    runnerVersion: string;
    testId: string;
    durationMs: number;
    stdout?: string;
  };
}

export interface TestFailEvidence {
  kind: "test-fail";
  schema: string;
  body: {
    runner: string;
    runnerVersion: string;
    testId: string;
    durationMs: number;
    stdout?: string;
    failureDetail?: string;
  };
}

export interface LlmProposalEvidence {
  kind: "llm-proposal";
  schema: string;
  body: {
    llm: string;
    llmVersion: string;
    promptCid: string; // hex32
    proposedIrFormula: string;
    confidence: number; // 0..1
    rationale?: string;
  };
}

export interface MutationWitnessEvidence {
  kind: "mutation-witness";
  schema: string;
  body: {
    testCid: string; // hex32
    mutationCid: string; // hex32
    failsOnOriginal: boolean;
    passesOnFixed: boolean;
  };
}

export interface WorkflowRunEvidence {
  kind: "workflow-run";
  schema: string;
  body: {
    workflowName: string;
    workflowCid: string; // hex32
    inputCanonicalForm: Record<string, unknown>;
    output: unknown;
  };
}

export interface LegacyWitnessEvidence {
  kind: "legacy-witness";
  schema: string;
  body: {
    rawWitness: string;
    legacyProducerId: string;
  };
}

export type EvidenceVariant =
  | Z3ModelEvidence
  | Z3UnsatEvidence
  | PatternMatchEvidence
  | TypeCheckPassEvidence
  | LintPassEvidence
  | TestPassEvidence
  | TestFailEvidence
  | LlmProposalEvidence
  | MutationWitnessEvidence
  | WorkflowRunEvidence
  | LegacyWitnessEvidence;

// ---------------------------------------------------------------------------
// ClaimEnvelope
// ---------------------------------------------------------------------------

/**
 * The universal claim envelope. Every memento produced by any producer
 * in any host language MUST conform to this schema.
 */
export interface ClaimEnvelope {
  /** Bump on incompatible wrapper change. Currently "1". */
  schemaVersion: "1";
  /**
   * sha256-prefix-16 of the canonical-encoded content identity.
   * 16 hex chars.
   */
  bindingHash: string;
  /**
   * sha256-prefix-16 of the canonical-encoded property identity.
   * 16 hex chars.
   */
  propertyHash: string;
  /** Producer's verdict on whether the property holds. */
  verdict: Verdict;
  /**
   * Versioned producer identity string. Format: `<name>@<version>`.
   * Examples: "z3-symbolic@4.13.4", "llm:claude-opus@4-7".
   */
  producedBy: string;
  /** ISO-8601 UTC timestamp. */
  producedAt: string;
  /**
   * CIDs of upstream mementos that fed this one. Empty for terminal
   * mementos. Sorted lexicographically.
   */
  inputCids: string[];
  /** Tagged-union evidence variant. */
  evidence: EvidenceVariant;
  /**
   * Optional ed25519 signature over the canonicalized envelope
   * (with `producerSignature` and `cid` elided). Base64-encoded.
   */
  producerSignature?: string;
  /**
   * sha256-prefix-32 of the canonicalized envelope (with `cid` and
   * `producerSignature` elided during hashing). 32 hex chars.
   */
  cid: string;
}
