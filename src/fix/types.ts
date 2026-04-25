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

/**
 * Thrown by runAgentInOverlay when the agent attempts to access a path outside
 * the overlay worktree. This is a safety violation — the prompt did not correctly
 * confine the agent to the overlay.
 *
 * IMPORTANT: this error is raised AFTER the agent has already run. The leaked
 * tool call may have already mutated files on disk. This error prevents the
 * resulting patch from being recorded, but does NOT undo any filesystem changes.
 * Callers should treat the overlay state as poisoned and close it.
 */
export class OverlayBypassError extends Error {
  constructor(
    /** The tool that was used (Edit, Write, Read, Bash). */
    public readonly toolName: string,
    /** The path Claude attempted to access (as reported in tool input). */
    public readonly claudePath: string,
    /** The overlay root that all tool paths should have been under. */
    public readonly overlayRoot: string,
  ) {
    super(
      `Claude attempted to use ${toolName} on a path outside the overlay. ` +
      `Path: ${claudePath}. Overlay root: ${overlayRoot}. ` +
      `This indicates the agent prompt did not correctly confine paths to the overlay. ` +
      `The overlay state may be poisoned — close and discard it.`,
    );
    this.name = "OverlayBypassError";
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

/** Draft artifacts produced by D2 in prDraftMode. */
export interface PrDraftArtifacts {
  /** Unified diff of the worktree commit vs its base. */
  patch: string;
  /** Markdown PR body derived from bundle metadata. */
  prBody: string;
}

/** Rollback audit for substrate bundles. */
export interface RollbackAudit {
  attempted: boolean;
  succeeded: boolean;
  detail: string;
}

/** Result of applying a bundle (D2). */
export interface ApplyResult {
  applied: boolean;
  failureReason?: string;
  prUrl?: string;
  /** SHA of the commit created in the apply worktree (or cherry-picked). */
  commitSha?: string;
  /** Gap IDs that were verified closed. Empty for non-gap-sourced signals. */
  closedGaps?: number[];
  /** Present in prDraftMode. */
  prDraft?: PrDraftArtifacts;
  /** Rollback status for substrate bundles. Only set when migration was attempted. */
  rollback?: RollbackAudit;
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
  /** Present when D2 ran. Exposes commitSha, prDraft, etc. */
  applyResult?: ApplyResult;
}

// ---------------------------------------------------------------------------
// LLM provider
// ---------------------------------------------------------------------------

export type { AgentRequestOptions, AgentResult } from "../llm/Provider.js";
import type { AgentRequestOptions, AgentResult } from "../llm/Provider.js";
import { mkdirSync, writeFileSync } from "fs";
import { dirname, join as pathJoin } from "path";
import { execFileSync as _execFileSync } from "child_process";

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
  /** Optional: agent-mode execution where the LLM edits files directly in cwd. */
  agent?(prompt: string, options: AgentRequestOptions): Promise<AgentResult>;
}

/** Canned agent response for StubLLMProvider. */
export interface StubAgentResponse {
  /** Prompt substring to match against. */
  matchPrompt: string;
  /** File edits the stub agent should apply to cwd. */
  fileEdits: { file: string; newContent: string }[];
  /** Optional final text response. */
  text?: string;
  /**
   * Optional tool use records to include in the AgentResult.
   * Use this to simulate tool calls with absolute paths for bypass-detection tests.
   * Each record must have id, name, input, isError, turn, ms at minimum.
   */
  toolUses?: AgentResult["toolUses"];
}

/** In-memory stub for tests. Caller supplies canned responses keyed by substring. */
export class StubLLMProvider implements LLMProvider {
  // Optional agent field — only assigned when agentResponses were provided.
  // Absence of the field is how candidateGen detects "no agent support."
  agent?: (prompt: string, options: AgentRequestOptions) => Promise<AgentResult>;

  constructor(
    private readonly responses: Map<string, string>,
    agentResponses?: StubAgentResponse[],
  ) {
    if (agentResponses && agentResponses.length > 0) {
      const cannedResponses = agentResponses;
      this.agent = async (prompt: string, options: AgentRequestOptions): Promise<AgentResult> => {
        const cannedResult = cannedResponses.find((r) => prompt.includes(r.matchPrompt));
        if (!cannedResult) {
          throw new Error(
            `StubLLMProvider.agent: no canned response for prompt. Registered match keys: ${cannedResponses.map((r) => r.matchPrompt).join(", ")}`,
          );
        }

        // Apply the canned file writes to cwd.
        for (const edit of cannedResult.fileEdits) {
          const absPath = pathJoin(options.cwd, edit.file);
          mkdirSync(dirname(absPath), { recursive: true });
          writeFileSync(absPath, edit.newContent, "utf-8");
        }

        // Collect modified tracked files.
        const changedTracked = (() => {
          try {
            return _execFileSync("git", ["diff", "--name-only"], { cwd: options.cwd, encoding: "utf-8" })
              .split("\n")
              .filter(Boolean);
          } catch {
            return [] as string[];
          }
        })();

        // Collect new untracked files the agent wrote.
        const newUntracked = (() => {
          try {
            return _execFileSync("git", ["ls-files", "--others", "--exclude-standard"], { cwd: options.cwd, encoding: "utf-8" })
              .split("\n")
              .filter(Boolean);
          } catch {
            return [] as string[];
          }
        })();

        const filesChanged = [...new Set([...changedTracked, ...newUntracked])];

        // Build the diff (use git add -N only for genuinely new files).
        const diff = (() => {
          try {
            if (newUntracked.length > 0) {
              _execFileSync("git", ["add", "-N", ...newUntracked], { cwd: options.cwd, stdio: "pipe" });
            }
            return _execFileSync("git", ["diff"], { cwd: options.cwd, encoding: "utf-8" });
          } catch {
            return "";
          }
        })();

        return {
          filesChanged,
          diff,
          text: cannedResult.text ?? "stub agent completed",
          turnsUsed: 1,
          toolUses: cannedResult.toolUses ?? [],
          thinkingBlocks: [],
          textBlocks: [],
        };
      };
    }
  }

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
   * For multi-hop reachability ("does X flow into primaryNode through any
   * number of intermediate variables"), use the `data_flow_reaches` relation
   * in DSL principles or query data_flow_transitive directly. The bipartite
   * limitation that previously prevented chains was fixed via init-edge
   * emission in src/sast/dataFlow.ts.
   */
  dataFlowAncestors: string[];
  /**
   * Node IDs consuming primaryNode's output (one-hop, via data_flow table).
   * For chained reachability use data_flow_transitive / data_flow_reaches.
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
 * One citation linking an SMT clause to a source-text quote in the bug report.
 * Produced by C1's LLM prompt (oracle #1.5 traceability check uses these).
 */
export interface InvariantCitation {
  /** The specific SMT clause being cited, e.g. "(= b 0)". */
  smt_clause: string;
  /** The verbatim quote from the bug report that justifies this clause. */
  source_quote: string;
}

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
  /**
   * Prose-to-clause citations for oracle #1.5 traceability check.
   * Present on novel-LLM-path invariants; null on principle-match-path invariants
   * (principle-match artifacts are vetted at C6 generation time).
   */
  citations?: InvariantCitation[] | null;
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

/** A test artifact (new test or modified test) that validates the fix. C5 fills this in. */
export interface TestArtifact {
  /** Path of the new test file inside the overlay worktree (e.g., "src/divide.regression.test.ts"). */
  testFilePath: string;
  /** Name of the vitest it(...) case. Stable, derived from signal + timestamp. */
  testName: string;
  /** The test source code (full file contents — the regression test standalone). */
  testCode: string;
  /** The Z3-witness-derived inputs that trigger the bug (materialized JS values). */
  witnessInputs: Record<string, unknown>;
  /** Oracle #9 lower half: test PASSED against the fixed code (in overlay). */
  passesOnFixedCode: boolean;
  /** Oracle #9 upper half: test FAILED against the original code (mutation check). */
  failsOnOriginalCode: boolean;
  /** Per-direction verdict detail (stdout/stderr, exit code) for the audit trail. */
  audit: {
    fixedRunStdout: string;
    fixedRunExitCode: number;
    originalRunStdout: string;
    originalRunExitCode: number;
    mutationApplied: boolean;
    mutationReverted: boolean;
  };
}

/**
 * C4: Kinds of complementary changes.
 *
 * This is a CLOSED set for v1. If new kinds are needed, they should go through
 * a registry similar to the intake adapter registry (see intakeRegistry.ts).
 * Registry migration is deferred to D1 per the artifact-kind registry plan.
 *
 * NOTE: "dominated_strip" (removing redundant checks in the dominance region) is
 * intentionally excluded from MVP per the C4 cut list.
 */
export type ComplementarySiteKind =
  | "adjacent_site_fix"   // same bug pattern at a different SAST node
  | "caller_update"       // call sites of the fixed function now need error handling
  | "data_flow_guard"     // a node feeding into the fix site lacks a guard upstream
  | "observability"       // add a metric/log at the site
  | "startup_assert";     // precondition guard at program boot

/** A complementary change that should accompany the primary fix. C4 fills this in. */
export interface ComplementaryChange {
  kind: ComplementarySiteKind;
  /** SAST node ID of the target site. */
  targetNodeId: string;
  /** The patch that implements the change. */
  patch: CodePatch;
  /** LLM's rationale. */
  rationale: string;
  /** Oracle #3 result: invariant holds at THIS site under the overlay. */
  verifiedAgainstOverlay: boolean;
  /** Z3 verdict from the overlay run for this specific site. */
  overlayZ3Verdict: "sat" | "unsat" | "unknown" | "error" | "bug_site_removed";
  /**
   * Priority bucket for sort order when the bundle assembles.
   * Lower number = higher priority. Caller updates ship first; observability last.
   */
  priority: number;
  audit: {
    siteKind: ComplementarySiteKind;
    discoveredVia: "principle_match" | "calls_table" | "data_flow_table" | "llm_reflection";
    z3RunMs: number;
  };
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
