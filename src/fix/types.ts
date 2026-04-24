/**
 * B1: Shared types for the fix loop.
 *
 * BugSignal is the normalized output of the intake layer.
 * The remaining types are stubs — B2/B3/B5 fill in their implementations.
 */

// ---------------------------------------------------------------------------
// B5: NotImplemented marker, audit trail, orchestrator result types
// ---------------------------------------------------------------------------

/** Thrown by stage stubs. Orchestrator catches this to record a graceful skip. */
export class NotImplementedError extends Error {
  constructor(
    public readonly stageId: string,
    message: string,
  ) {
    super(message);
    this.name = "NotImplementedError";
  }
}

/** One entry in the orchestrator's audit trail. */
export interface AuditEntry {
  /** Stage label: "C1", "C2", ..., "D3", or "orchestrator". */
  stage: string;
  kind: "start" | "complete" | "error" | "skipped";
  detail: string;
  timestamp: number;
  /** Each stage may attach its own extra data. */
  metadata?: Record<string, unknown>;
}

/** Capability spec for substrate-extension bundles (C6 → D1). stub — D1 will refine. */
export interface CapabilitySpec {
  capabilityName: string;
  schemaTs: string;
  migrationSql: string;
  extractorTs: string;
  extractorTestsTs: string;
  registryRegistration: string;
  positiveFixtures: { source: string; expectedRowCount: number }[];
  negativeFixtures: { source: string; expectedRowCount: 0 }[];
  rationale: string;
}

/** Principle candidate — tagged union per substrate-extension path (C6 output). */
export type PrincipleCandidate =
  | {
      kind: "principle";
      name: string;
      dslSource: string;
      smtTemplate: string;
      teachingExample: { domain: string; explanation: string; smt2: string };
      adversarialValidation: { validatorModel: string; result: "pass" | "fail"; evidence: string }[];
      latentSiteMatches: { nodeId: string; file: string; line: number }[];
    }
  | {
      kind: "principle_with_capability";
      name: string;
      dslSource: string;
      smtTemplate: string;
      teachingExample: { domain: string; explanation: string; smt2: string };
      adversarialValidation: { validatorModel: string; result: "pass" | "fail"; evidence: string }[];
      latentSiteMatches: { nodeId: string; file: string; line: number }[];
      capabilitySpec: CapabilitySpec;
    };

/**
 * A scratch-worktree overlay. Created from a git ref. Has its own SAST
 * DB. Supports patch application + selective re-index. Closed via
 * closeOverlay() (removes the worktree + deletes the scratch DB).
 */
export interface OverlayHandle {
  /** Absolute filesystem path to the scratch worktree. */
  worktreePath: string;
  /** Absolute path to the scratch SAST DB (a separate sqlite file — NOT the main DB). */
  sastDbPath: string;
  /** Open Drizzle Db handle against the scratch SAST DB. */
  sastDb: Db;
  /** Git ref the overlay was created from (branch name, commit SHA, or "HEAD"). */
  baseRef: string;
  /** Files that have been modified in the overlay, relative to worktreePath. */
  modifiedFiles: Set<string>;
  /** Whether the overlay has been closed (subsequent operations throw). */
  closed: boolean;
}

/**
 * One file edit within a CodePatch.
 * C3 produces these; overlay.ts consumes them.
 */
export interface CodePatchFileEdit {
  /** Path relative to the overlay's worktreePath. */
  file: string;
  /** NEW FULL file contents after the patch. (v1 MVP: whole-file rewrite; diff/unified-format come later.) */
  newContent: string;
}

/**
 * A code patch to apply to an overlay.
 *
 * v1.1 multi-file: each CodePatch carries one or more file edits.
 * Single-file patches have exactly one entry in fileEdits.
 * C3 produces CodePatch objects; overlay.ts consumes them.
 */
export interface CodePatch {
  /** The files this patch modifies. A single-file patch has one entry. */
  fileEdits: CodePatchFileEdit[];
  /** Human-readable description of what the patch does. */
  description: string;
}

/** Result of applying a bundle (D2). stub — D2 will refine. */
export interface ApplyResult {
  applied: boolean;
  failureReason?: string;
  prUrl?: string;
}

/** Fix bundle assembled by D1. stub shape — D1 will populate all fields. */
export interface FixBundle {
  bundleId: number;
  bundleType: "fix" | "substrate";
  bugSignal: BugSignal;
  plan: RemediationPlan;
  artifacts: {
    primaryFix: FixCandidate | null;
    complementary: ComplementaryChange[];
    test: TestArtifact | null;
    principle: PrincipleCandidate | null;
    capabilitySpec: CapabilitySpec | null;
  };
  coherence: {
    sastStructural: boolean;
    z3SemanticConsistency: boolean;
    fullSuiteGreen: boolean;
    noNewGapsIntroduced: boolean;
    migrationSafe: boolean | null;
    crossCodebaseRegression: boolean | null;
    extractorCoverage: boolean | null;
    substrateConsistency: boolean | null;
    principleNeedsCapability: boolean | null;
  };
  confidence: number;
  auditTrail: AuditEntry[];
}

/** Orchestrator top-level result (B5 output). */
export interface FixLoopResult {
  bundle: FixBundle | null;
  applied: boolean;
  auditTrail: AuditEntry[];
  reason?: string;
}

// ---------------------------------------------------------------------------
// LLM provider
// ---------------------------------------------------------------------------

export interface LLMProvider {
  /**
   * Send a prompt; get a string response.
   * Tier-aware for adversarial validation (C6) but not required to differ for v1.
   */
  complete(params: {
    prompt: string;
    model?: "haiku" | "sonnet" | "opus";
    schema?: object;
  }): Promise<string>;
}

/** In-memory stub for tests. Caller supplies canned responses keyed by substring. */
export class StubLLMProvider implements LLMProvider {
  constructor(private readonly responses: Map<string, string>) {}

  async complete(params: { prompt: string; model?: string }): Promise<string> {
    for (const [key, value] of this.responses) {
      if (params.prompt.includes(key)) return value;
    }
    throw new Error(
      `stub LLM: no canned response for prompt containing any of: ${[...this.responses.keys()].join(", ")}`,
    );
  }
}

// ---------------------------------------------------------------------------
// BugSignal — normalized bug report (B1 output)
// ---------------------------------------------------------------------------

export interface CodeReference {
  file: string;
  line?: number;
  function?: string;
}

/**
 * Normalized bug report produced by intake.
 * source is a plain string resolved via the intake adapter registry — no closed enum.
 */
export interface BugSignal {
  /** Adapter name used to parse this signal. No closed enum — resolved via registry. */
  source: string;
  rawText: string;
  /** One-sentence summary, typically LLM-extracted. */
  summary: string;
  /** Human-readable description of what goes wrong. */
  failureDescription: string;
  fixHint?: string;
  codeReferences: CodeReference[];
  bugClassHint?: string;
}

// ---------------------------------------------------------------------------
// Downstream stubs (B2/B3/B5 implement these)
// ---------------------------------------------------------------------------

/** A precise location in source code where a bug is likely manifesting. B2 fills this in. */
export interface BugLocus {
  // Human-readable position (from B1's stub)
  file: string;
  line: number;
  function?: string;
  /** Confidence 0..1 that this locus is the root cause. */
  confidence: number;

  // SAST-structural fields (new in B2)
  /** Node ID most likely to be the bug site — a concrete SAST node. */
  primaryNode: string;
  /** Node ID of the enclosing function-like node (FunctionDeclaration / ArrowFunction / etc.).
   * If the bug site is at module top-level, this equals the file's root node ID. */
  containingFunction: string;
  /**
   * Node IDs of callers and callees of the containing function.
   * V1: intra-file only. Cross-file resolution requires joining across multiple SAST builds
   * which we defer until C4 needs it. Cap: 50 entries max.
   */
  relatedFunctions: string[];
  /**
   * Node IDs whose values flow INTO primaryNode (one-hop, via data_flow table).
   *
   * KNOWN LIMITATION: data_flow_transitive is bipartite (decls as from_node, use-site
   * identifiers as to_node). Transitive closure equals direct edges — no chains form.
   * When C4 (complementary-site discovery) needs chained reachability, revisit the
   * edge-shape redesign described in src/sast/dataFlow.ts (commit 9f4c220).
   */
  dataFlowAncestors: string[];
  /**
   * Node IDs consuming primaryNode's output (one-hop, via data_flow table).
   * Same bipartite limitation as dataFlowAncestors above.
   */
  dataFlowDescendants: string[];
  /** Node IDs of everything primaryNode dominates (via dominance table). */
  dominanceRegion: string[];
  /** Node IDs of everything primaryNode post-dominates (via post_dominance table). */
  postDominanceRegion: string[];
}

/**
 * Thrown by formulateInvariant (C1) when it cannot produce a valid, Z3-sat-checked invariant.
 * The orchestrator catches this and records a graceful skip.
 */
export class InvariantFormulationFailed extends Error {
  constructor(message: string) {
    super(message);
    this.name = "InvariantFormulationFailed";
  }
}

/**
 * Re-export SmtBinding from contracts.ts for use in InvariantClaim.
 * Contracts.ts only imports fs/path/crypto — no circular risk.
 */
import type { SmtBinding } from "../contracts.js";
import type { Db } from "../db/index.js";
export type SmtBindingRef = SmtBinding;

/**
 * A formal claim about what invariant the code is violating.
 * C1 (formulateInvariant) produces this; oracle #1 has already confirmed SAT before returning.
 */
export interface InvariantClaim {
  /**
   * The principle this invariant was derived from, or null for LLM-generated novel invariants.
   */
  principleId: string | null;
  /** Human-readable description of what property must hold. */
  description: string;
  /**
   * The concrete SMT-LIB assertion expressing the NEGATION of the goal (the violation state).
   * Must be check-sat-able by Z3 directly.
   * Pre-fix: check-sat returns "sat" (bug is reachable).
   * Post-fix: check-sat returns "unsat" (bug is unreachable).
   */
  formalExpression: string;
  /** Per-constant bindings mapping SMT variables to source positions. */
  bindings: SmtBindingRef[];
  /** Complexity score for downstream ranking. Computed by proofComplexity() from verifier.ts. */
  complexity: number;
  /** Z3 witness from oracle #1 run (the sat-ness proves the bug is real). */
  witness: string | null;
}

/** A concrete code change proposed to fix the bug. C3 fills this in. */
export interface FixCandidate {
  /** The patch (CodePatch). v1 MVP allows multi-file via fileEdits. */
  patch: CodePatch;
  /** LLM's rationale for why this patch fixes the bug. */
  llmRationale: string;
  /** LLM self-reported confidence 0..1. */
  llmConfidence: number;
  /** Oracle #2 result: does C1's invariant now hold under the overlay? (Z3 negated-goal unsat.) */
  invariantHoldsUnderOverlay: boolean;
  /** Z3 verdict from the overlay run — "unsat" means invariant holds (fix is real). */
  overlayZ3Verdict: "sat" | "unsat" | "unknown" | "error";
  /**
   * Audit of the C3 run: what the LLM said, what Z3 said, what the overlay contained.
   * Note: overlayClosed is always false when C3 returns — the orchestrator owns the overlay lifecycle.
   */
  audit: {
    overlayCreated: boolean;
    patchApplied: boolean;
    overlayReindexed: boolean;
    z3RunMs: number;
    /** Always false from C3's perspective — orchestrator closes the overlay after C3 returns. */
    overlayClosed: boolean;
  };
}

/** A test artifact (new test or modified test) that validates the fix. B5 fills this in. */
export interface TestArtifact {
  file: string;
  testName: string;
  body: string;
}

/** A complementary change that should accompany the primary fix. B5 fills this in. */
export interface ComplementaryChange {
  description: string;
  file?: string;
  patch?: string;
}

/**
 * Artifact descriptor. kind is a plain string resolved downstream (in B5/D1).
 * No closed union — artifact kinds will migrate to their own registry at D1.
 * For now, extra fields are optional and untyped beyond what B3 emits.
 */
export interface PlannedArtifact {
  kind: string;              // code_patch, regression_test, startup_assert, etc.
  envVar?: string;           // for startup_assert
  site?: string;             // for error_handler
  bugClassName?: string;     // for principle_candidate
  rationale?: string;
}

/**
 * Top-level container returned by the full fix loop.
 * B3 fills primaryLayer, secondaryLayers, artifacts, rationale.
 * B5 fills candidates, tests, complementary.
 */
export interface RemediationPlan {
  signal: BugSignal;
  locus: BugLocus | null;

  /** Primary layer name — resolved via registry; no closed union. */
  primaryLayer: string;

  /** Additional layers the fix may need. */
  secondaryLayers: string[];

  /** Proposed artifacts by kind. */
  artifacts: PlannedArtifact[];

  /** LLM's rationale for this plan. Audit log. */
  rationale: string;

  // Legacy fields from B1 stub — kept for backward compat with B5's eventual use
  loci?: BugLocus[];
  claims?: InvariantClaim[];
  candidates?: FixCandidate[];
  tests?: TestArtifact[];
  complementary?: ComplementaryChange[];
}
