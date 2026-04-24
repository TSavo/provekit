/**
 * B1: Shared types for the fix loop.
 *
 * BugSignal is the normalized output of the intake layer.
 * The remaining types are stubs — B2/B3/B5 fill in their implementations.
 */

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
 * A formal claim about what invariant the code is violating.
 * B3 (classify) populates this from DSL principles.
 */
export interface InvariantClaim {
  principleId: string;
  description: string;
  /** Optional SMT/DSL expression encoding the invariant. */
  formalExpression?: string;
}

/** A concrete code change proposed to fix the bug. B5 fills this in. */
export interface FixCandidate {
  file: string;
  patch: string;
  rationale: string;
  confidence: number;
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
 * Top-level container returned by the full fix loop.
 * B2/B3/B5 each add their sections; this stub defines the shape.
 */
export interface RemediationPlan {
  signal: BugSignal;
  loci: BugLocus[];
  claims: InvariantClaim[];
  candidates: FixCandidate[];
  tests: TestArtifact[];
  complementary: ComplementaryChange[];
}
