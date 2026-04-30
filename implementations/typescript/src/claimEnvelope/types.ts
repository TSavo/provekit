/**
 * Universal claim envelope — TypeScript types.
 *
 * Spec: protocol/specs/2026-04-29-universal-claim-envelope.md
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
  schema: string; // self-identifying CID of the variant schema definition (e.g. "blake3-512:...")
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
    rulesetHash: string; // self-identifying hash
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
    promptCid: string; // self-identifying hash
    proposedIrFormula: string;
    confidence: number; // 0..1
    rationale?: string;
  };
}

export interface MutationWitnessEvidence {
  kind: "mutation-witness";
  schema: string;
  body: {
    testCid: string; // self-identifying hash
    mutationCid: string; // self-identifying hash
    failsOnOriginal: boolean;
    passesOnFixed: boolean;
  };
}

export interface WorkflowRunEvidence {
  kind: "workflow-run";
  schema: string;
  body: {
    workflowName: string;
    workflowCid: string; // self-identifying hash
    inputCanonicalForm: Record<string, unknown>;
    output: unknown;
  };
}

/**
 * BridgeEvidence — declares that a host-language symbol is the surface
 * realization of a deeper-layer published contract. The bridge composes
 * by hash; it does NOT redefine the contract.
 */
export interface BridgeEvidence {
  kind: "bridge";
  schema: string;
  body: {
    /** Symbol name in the host language (e.g., "global.parseInt"). */
    sourceSymbol: string;
    /** Description of the host-language layer (e.g., "TS-kit@1.0"). */
    sourceLayer: string;
    /** The CID of the deeper-layer contract memento this bridges to. */
    targetContractCid: string;
    /** Description of the deeper layer (e.g., "V8@12.4 parseInt"). */
    targetLayer: string;
    /** IR argument sorts of the bridged primitive (each a SortRef). */
    irArgSorts: unknown[];
    /** IR return sort of the bridged primitive (a SortRef). */
    irReturnSort: unknown;
    /** Optional notes about the bridge. */
    notes?: string;
  };
}

// ---------------------------------------------------------------------------
// Authoring blocks for ContractEvidence (typed union per spec).
// ---------------------------------------------------------------------------

export interface KitAuthorAuthoring {
  producerKind: "kit-author";
  author: string;
  note?: string;
}

export interface LiftAuthoring {
  producerKind: "lift";
  lifter: string;
  evidence: "tests" | "types" | "docs" | "symbolic-exec";
  sourceCid?: string;
}

export interface LlmAuthoring {
  producerKind: "llm";
  llm: string;
  llmVersion: string;
  promptCid: string;
  confidence: number;
  rationale?: string;
}

export type ContractAuthoring = KitAuthorAuthoring | LiftAuthoring | LlmAuthoring;

/**
 * ContractEvidence — a behavior contract for a function-shaped binding.
 * Carries any combination of precondition (pre), postcondition (post),
 * and inductive invariant (inv); at least one MUST be present.
 *
 * Per the v1.1 protocol cut, this replaces the previous
 * PropertyEvidence shape entirely.
 */
export interface ContractEvidence {
  kind: "contract";
  schema: string;
  body: {
    contractName: string;
    outBinding: string;
    /** Optional precondition formula (IR-JSON). */
    pre?: unknown;
    /** Optional postcondition formula (IR-JSON). */
    post?: unknown;
    /** Optional inductive invariant formula (IR-JSON). */
    inv?: unknown;
    /** DERIVED: computeCid(canonical(pre)) — present iff pre is present. */
    preHash?: string;
    /** DERIVED: computeCid(canonical(post)) — present iff post is present. */
    postHash?: string;
    /** DERIVED: computeCid(canonical(inv)) — present iff inv is present. */
    invHash?: string;
    /** Authoring provenance — typed union per spec. */
    authoring: ContractAuthoring;
  };
}

/**
 * ImplicationEvidence — a signed proof witness that one IR formula
 * universally implies another. Cached implication mementos are how the
 * handshake algorithm shortcuts a future solver call.
 */
export interface ImplicationEvidence {
  kind: "implication";
  schema: string;
  body: {
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
  };
}

/**
 * ExtensionDeclarationEvidence — wraps an IR extension declaration
 * (sort / predicate / ctor introduction) per the IR extension protocol
 * (protocol/specs/2026-04-30-ir-extension-protocol.md).
 *
 * Today's extensions live in ir/extensions/registry.ts with their own
 * signing path; this variant brings them into the memento envelope so
 * they can ship in `.proof` catalogs alongside bridges + properties.
 * Consumers (kitDiscovery) dispatch on evidence.kind === "extension-
 * declaration" to register them in the local extension registry.
 *
 * The wrapped declaration is itself the canonical IR extension shape;
 * the envelope's producerSignature replaces the declaration's
 * embedded signer/signature fields (which become redundant).
 */
export interface ExtensionDeclarationEvidence {
  kind: "extension-declaration";
  schema: string;
  body: {
    /**
     * The IR extension declaration. Conforms to ExtensionDeclaration in
     * src/ir/extensions/registry.ts (sort | predicate | ctor variants).
     * Stored as a JSON value; the embedded signer/signature fields (if
     * any) are unused — the envelope's producerSignature is the
     * authority.
     */
    declaration: unknown;
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
  | BridgeEvidence
  | ContractEvidence
  | ImplicationEvidence
  | ExtensionDeclarationEvidence;

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
