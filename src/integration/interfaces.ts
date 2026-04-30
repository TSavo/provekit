/**
 * Channel-2 integration interfaces.
 *
 * The interfaces here are the named, documented boundaries an integrator can
 * swap to plug their own implementation behind ProvekIt's pipeline. Each
 * interface is one job — no kitchen-sink contracts. The reference
 * implementations under `./reference/` delegate to the existing module-scoped
 * code in `src/sast/`, `src/fix/`, etc.; they exist so the public surface has
 * a stable, documented shape that integrators can target.
 *
 * Spec citations:
 *   protocol/specs/2026-04-27-constraint-driven-development.md
 *     "The pipeline as a composition of swappable processes"
 *     "Part B as a plugin: the constraint-minting interface"
 *   protocol/specs/2026-04-27-standing-invariant-runtime.md
 *     "Distribution surface" / "What integrators can swap"
 *
 * Design notes:
 *
 * - Existing types are reused wherever they already exist. We do not redefine
 *   `LLMProvider`, `OverlayHandle`, `CodePatch`, etc. — they are the
 *   already-stable contracts and the interfaces here re-export them.
 *
 * - "Stage" is a naming convention, not a literal supertype. Each stage in
 *   `src/fix/stages/` exports its own typed function `(input, deps)`. A
 *   shared `Stage` interface either ends up `any`-typed or a giant union.
 *   The orchestrator is artifact-stream collector; each stage call returns
 *   `Artifact | null`.
 *
 * - `AstGraph` is intentionally minimal. The fix loop reads SAST data through
 *   the SQLite substrate (tables: nodes, node_children, capabilities, ...);
 *   it does not consume a richer graph struct. The Parser interface is the
 *   thin facade over `buildSASTForFile`.
 */

// ---------------------------------------------------------------------------
// Re-exports of existing public contracts.
// Keep the integrator-facing surface in one place.
// ---------------------------------------------------------------------------

export type {
  LLMProvider,
  LLMRequestOptions,
  LLMResponse,
  LLMStreamEvent,
  AgentRequestOptions,
  AgentResult,
  ToolUseRecord,
} from "../llm/Provider.js";

export type {
  OverlayHandle,
  CodePatch,
  CodePatchFileEdit,
  BugSignal,
  BugLocus,
  CodeReference,
  InvariantClaim,
  FixCandidate,
  TestArtifact,
  PrincipleCandidate,
  ComplementaryChange,
  FixBundle,
  RemediationPlan,
} from "../fix/types.js";

export type { StoredInvariant } from "../fix/runtime/invariantStore.js";

// ---------------------------------------------------------------------------
// Parser
//
// Job: parse source text into a typed AST graph rooted at one file.
//
// The reference implementation is `ReferenceTsMorphParser`, which delegates
// to `buildSASTForFile` (uses ts-morph + the SQLite substrate). Cross-language
// integrators implement Parser by emitting AstGraph nodes with positions; the
// downstream substrate import is shared.
// ---------------------------------------------------------------------------

export interface AstNode {
  /** Stable id of the node within the graph. */
  id: string;
  /** Kind label, e.g., "FunctionDeclaration", "BinaryExpression". Free-form string. */
  kind: string;
  /** Byte-offset of the start of the node's full text. */
  start: number;
  /** Byte-offset of one past the end. */
  end: number;
  /** 1-based line number of the start position. */
  line: number;
  /** 0-based column of the start position. */
  column: number;
}

export interface AstEdge {
  /** Parent node id. */
  parentId: string;
  /** Child node id. */
  childId: string;
  /** 0-based child order under the parent. */
  childOrder: number;
}

export interface AstGraph {
  /** Absolute path of the parsed file. */
  filePath: string;
  /** Root node id. */
  rootId: string;
  /** All nodes, including the root. */
  nodes: AstNode[];
  /** Parent-child edges. */
  edges: AstEdge[];
}

export interface Parser {
  /** Human-readable name of this implementation, e.g. "ts-morph" / "tree-sitter-rust". */
  readonly name: string;
  /**
   * Parse `source` into an AstGraph. `filePath` is recorded on the graph and
   * may also be used by the implementation to resolve language extension or
   * tsconfig.
   */
  parse(source: string, filePath: string): AstGraph;
}

// ---------------------------------------------------------------------------
// Substrate
//
// Job: store an AST graph and answer structural queries against it.
//
// The reference implementation is `ReferenceSqliteSubstrate` (Drizzle on top
// of better-sqlite3). The interface is intentionally generic so future
// implementations (e.g., an in-memory substrate for IDE-side incremental
// reindex) can plug in. v1 surface is the node/edge primitives plus a
// minimal lookup-by-position used by the fix-loop verifier path.
// ---------------------------------------------------------------------------

export type NodeId = string;

export interface SubstrateQueryByPosition {
  filePath: string;
  /** 1-based line number; column optional. */
  line: number;
  column?: number;
}

export interface Substrate {
  readonly name: string;
  /** Insert a node. Returns its assigned id (typically equal to node.id). */
  addNode(node: AstNode): NodeId;
  /** Insert an edge. */
  addEdge(from: NodeId, to: NodeId, slot: string): void;
  /**
   * Look up the node id whose span covers the given (file, line[, column]).
   * Returns null when no node spans the position.
   */
  query(q: SubstrateQueryByPosition): NodeId | null;
}

// ---------------------------------------------------------------------------
// Sandbox
//
// Job: provide an isolated workspace where a candidate fix can be applied,
//      reindexed, and tested without affecting the user's main checkout.
//
// Reference: git worktree on local disk (`ReferenceGitWorktreeSandbox`).
// Alternatives an integrator could plug in: container, microVM, IDE
// in-process VFS, remote workspace.
// ---------------------------------------------------------------------------

export interface RunResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

export interface Sandbox {
  readonly name: string;
  /** Open a sandbox over the given project root. Returns a handle. */
  open(projectRoot: string, opts?: { locusFile?: string }): Promise<import("../fix/types.js").OverlayHandle>;
  /** Apply a patch to the sandbox. */
  apply(handle: import("../fix/types.js").OverlayHandle, patch: import("../fix/types.js").CodePatch): Promise<void>;
  /** Run a command inside the sandbox. */
  runCommand(handle: import("../fix/types.js").OverlayHandle, cmd: string[]): RunResult;
  /** Tear down the sandbox. */
  close(handle: import("../fix/types.js").OverlayHandle): Promise<void>;
}

// ---------------------------------------------------------------------------
// BuildTestRunner
//
// Job: run a project's tests against the sandbox.
//
// The reference implementation is `ReferenceVitestPnpmRunner`, which reuses
// the existing test-runner registry (vitest, jest, mocha, node-test, none).
// ---------------------------------------------------------------------------

export interface TestTarget {
  /** Project-relative path of a single test file, or null for full suite. */
  testFilePath: string | null;
  /** Optional: pin to a specific runner descriptor name. Default: auto-detect. */
  runnerName?: string;
}

export interface BuildTestRunner {
  readonly name: string;
  /** Run the requested target inside the sandbox; return the raw process result. */
  run(sandbox: import("../fix/types.js").OverlayHandle, target: TestTarget): RunResult;
}

// ---------------------------------------------------------------------------
// SourcePatternScanner
//
// Job: answer "does this source span match a named pattern?"
//
// The reference is `ReferenceTypeScriptPatternScanner`, which currently
// covers the `asc(`/`desc(` patterns used by pathChecker for the order kind.
// Per-language scanners can plug in for additional kinds.
// ---------------------------------------------------------------------------

export interface PatternQuery {
  /** Pattern kind, e.g. "order/asc-or-desc", "taint/raw-eval". */
  kind: string;
  /** Optional regex flavour or sub-pattern selector. */
  selector?: string;
}

export interface ScanContext {
  filePath: string;
  /** Source text of the file (or relevant region). */
  source: string;
  /** 1-based line number to scan. */
  line: number;
  /** Optional ± line window to broaden the match. */
  windowLines?: number;
}

export interface SourcePatternScanner {
  readonly name: string;
  /**
   * Whether the requested pattern matches at the given line/window.
   * Implementations may also expose richer details via a sibling helper, but
   * the boolean answer is the contract.
   */
  scan(ctx: ScanContext, query: PatternQuery): boolean;
}

// ---------------------------------------------------------------------------
// PatchApplicator
//
// Job: take a CodePatch and write the file edits onto the sandbox tree.
//
// The reference implementation `ReferenceGitApplyApplicator` writes file
// contents directly (the existing `applyPatchToOverlay` flow) plus stages
// them via `git add -N` so subsequent diffs include new files.
// ---------------------------------------------------------------------------

export interface ApplyPatchResult {
  /** Whether all file edits were written successfully. */
  ok: boolean;
  /** Files that were written. */
  filesWritten: string[];
  /** Diagnostic message (set when ok is false). */
  detail?: string;
}

export interface PatchApplicator {
  readonly name: string;
  apply(
    sandbox: import("../fix/types.js").OverlayHandle,
    patch: import("../fix/types.js").CodePatch,
  ): Promise<ApplyPatchResult>;
}

// ---------------------------------------------------------------------------
// Stage
//
// "Stage" is a naming convention. Each stage exports its own typed function
// `runX(input, deps): Promise<Artifact | null>`. There is no shared supertype
// — see file header for the rationale. The two helper types below describe
// the documented `(input, deps)` partition without forcing a literal
// interface inheritance on the existing stages.
// ---------------------------------------------------------------------------

export interface StageDependencies {
  llm?: import("../llm/Provider.js").LLMProvider;
  sandbox?: Sandbox;
  parser?: Parser;
  substrate?: Substrate;
  scanner?: SourcePatternScanner;
  testRunner?: BuildTestRunner;
  applicator?: PatchApplicator;
  /** Logger surface — passed through opaquely. */
  logger?: unknown;
  /** Db handle — typed in the stage signatures themselves. */
  db?: unknown;
}

/**
 * Documentation type. Stage functions are NOT required to extend this; they
 * each declare their own input shape and dep bundle in their own signature.
 * The orchestrator types its calls against the concrete signatures.
 */
export type StageInput = Record<string, unknown>;

// ---------------------------------------------------------------------------
// Artifact stream
//
// The fix loop emits a sequence of typed artifacts. Each variant carries its
// own payload and a destination contract — flushed unconditionally after the
// stages finish. Not every stage emits every kind; not every kind survives
// to the bundle. The flush step is what writes them to disk (or the bundle).
//
// CRITICAL: cross-stage flush gating is removed. A failure in C6 must not
// suppress C1's invariant artifact. The orchestrator collects every artifact
// produced and flushes them all at the end.
// ---------------------------------------------------------------------------

import type {
  InvariantClaim as _InvariantClaim,
  CodePatch as _CodePatch,
  TestArtifact as _TestArtifact,
  PrincipleCandidate as _PrincipleCandidate,
  FixBundle as _FixBundle,
  BugSignal as _BugSignal,
  BugLocus as _BugLocus,
} from "../fix/types.js";
import type {
  IntentReport as _IntentReport,
} from "../fix/intake/retrospective.js";
import type {
  StoredInvariant as _StoredInvariant,
} from "../fix/runtime/invariantStore.js";

/**
 * An invariant artifact. Flush destination: `.provekit/invariants/<id>.json`
 * via writeInvariant. Carries the supporting bug signal + locus + test, which
 * are needed to materialize a StoredInvariant at flush time.
 */
export interface InvariantArtifact {
  kind: "invariant";
  claim: _InvariantClaim;
  signal: _BugSignal;
  locus: _BugLocus;
  test: _TestArtifact | null;
  patchSha: string | null;
  /** Already-built StoredInvariant, when the producer has materialized it. */
  stored?: _StoredInvariant;
}

/**
 * A patch artifact. Flush destination: bundle assembly + (when applied) git
 * commit on the apply branch. Stand-alone in artifact-stream form so a
 * downstream failure does not lose the patch.
 */
export interface PatchArtifact {
  kind: "patch";
  patch: _CodePatch;
  rationale: string;
  /** B3 source — "library" for mechanical fixes, "llm" otherwise. */
  source?: "library" | "llm";
}

/**
 * A regression test artifact. Flush destination: bundle assembly + on-apply
 * commit (the test source lands in the test directory of the project).
 */
export interface RegressionTestArtifact {
  kind: "regression_test";
  test: _TestArtifact;
}

/**
 * A principle artifact. Flush destination: bundle assembly + (when applied)
 * `.provekit/principles/<id>.json`. Keeps the alternate-shapes layer.
 */
export interface PrincipleArtifact {
  kind: "principle";
  principle: _PrincipleCandidate;
  alternateShapes?: _PrincipleCandidate[];
}

/**
 * An IntentReport artifact (B0 retrospective intake). Flush destination:
 * `.provekit/intent/<commit-sha>.json`.
 */
export interface IntentReportArtifact {
  kind: "intent_report";
  report: _IntentReport;
  /** When known, the commit SHA the intent report describes. */
  commitSha?: string;
}

/**
 * A fix-bundle artifact. Flush destination: persistence via D1's existing
 * bundlePersistence path + audit trail.
 */
export interface BundleArtifact {
  kind: "bundle";
  bundle: _FixBundle;
}

/**
 * An oracle report artifact. Auxiliary; carries verdict metadata that
 * integrators may want without consuming the full bundle.
 */
export interface OracleReportArtifact {
  kind: "oracle_report";
  oracleId: string;
  verdict: "pass" | "fail" | "skip";
  detail: string;
}

export type Artifact =
  | InvariantArtifact
  | PatchArtifact
  | RegressionTestArtifact
  | PrincipleArtifact
  | IntentReportArtifact
  | BundleArtifact
  | OracleReportArtifact;

/**
 * Type guard helpers. Exported because integrators consuming the artifact
 * stream will want them.
 */
export const isInvariantArtifact = (a: Artifact): a is InvariantArtifact => a.kind === "invariant";
export const isPatchArtifact = (a: Artifact): a is PatchArtifact => a.kind === "patch";
export const isRegressionTestArtifact = (a: Artifact): a is RegressionTestArtifact => a.kind === "regression_test";
export const isPrincipleArtifact = (a: Artifact): a is PrincipleArtifact => a.kind === "principle";
export const isIntentReportArtifact = (a: Artifact): a is IntentReportArtifact => a.kind === "intent_report";
export const isBundleArtifact = (a: Artifact): a is BundleArtifact => a.kind === "bundle";
export const isOracleReportArtifact = (a: Artifact): a is OracleReportArtifact => a.kind === "oracle_report";
