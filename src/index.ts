// Channel 2 — Library entry points for IDE / agent runtime / platform integration.
//
// This file is the SINGLE public entry point for the `provekit` package.
// Everything exported here is semver-committed: backward-compatibility on
// these names + signatures within the 0.x major. Anything NOT exported from
// this file is internal and may break without notice.
//
// The four canonical entry points named in
// protocol/specs/2026-04-27-standing-invariant-runtime.md (Distribution surface,
// Artifact 2) are:
//
//   runFixLoop      — the full LLM-touching pipeline
//   verifyAll       — the standing-runtime gate (Z3 + path enumerator, no LLM)
//   extractIntent   — B0 retrospective intake
//   readInvariants
//   writeInvariant  — constraint store I/O
//
// The CLI binary lives at src/cli.ts and consumes the same surface.
//
// See docs/library-integration.md for worked integration examples.

// ---------------------------------------------------------------------------
// Pipeline (full LLM-touching loop)
// ---------------------------------------------------------------------------
export { runFixLoop } from "./fix/orchestrator.js";
export type { RunFixLoopArgs, FixLoopResultWithArtifacts } from "./fix/orchestrator.js";

// ---------------------------------------------------------------------------
// Individual pipeline stages (channel 2 — call them directly without the
// orchestrator).
//
// Per protocol/specs/2026-04-27-constraint-driven-development.md "The pipeline
// as a composition of swappable processes": every stage's input/output type
// is a public, documented contract. The library entry points expose stages
// individually, not just runFixLoop. An integrator who wants only our C5
// imports `generateRegressionTest` directly.
// ---------------------------------------------------------------------------
export { recognize } from "./fix/stages/recognize.js";
export { formulateInvariant } from "./fix/stages/formulateInvariant.js";
export { openOverlay } from "./fix/stages/openOverlay.js";
export { generateFixCandidate } from "./fix/stages/generateFixCandidate.js";
export { generateComplementary } from "./fix/stages/generateComplementary.js";
export { generateRegressionTest } from "./fix/stages/generateRegressionTest.js";
export { generatePrincipleCandidate } from "./fix/stages/generatePrincipleCandidate.js";
export { assembleBundle } from "./fix/stages/assembleBundle.js";
export { applyBundle } from "./fix/stages/applyBundle.js";
export { learnFromBundle } from "./fix/stages/learnFromBundle.js";
export { investigate } from "./fix/stages/investigate.js";
export type { RecognizeResult } from "./fix/stages/recognize.js";
export type { InvestigateReport } from "./fix/stages/investigate.js";
export type { GenerateFixCandidateDeps } from "./fix/stages/generateFixCandidate.js";
export type { GenerateRegressionTestDeps } from "./fix/stages/generateRegressionTest.js";

// ---------------------------------------------------------------------------
// Channel 2 — Integration interfaces and reference implementations.
//
// The named, documented boundary contracts an integrator can swap behind
// ProvekIt's pipeline. Each interface is one job. Reference classes are
// thin wrappers over the existing module-scoped code.
// ---------------------------------------------------------------------------
export type {
  Parser,
  Substrate,
  Sandbox,
  BuildTestRunner,
  SourcePatternScanner,
  PatchApplicator,
  AstGraph,
  AstNode,
  AstEdge,
  NodeId,
  RunResult,
  TestTarget,
  PatternQuery,
  ScanContext,
  ApplyPatchResult,
  StageDependencies,
  StageInput,
  Artifact,
  InvariantArtifact,
  PatchArtifact,
  RegressionTestArtifact,
  PrincipleArtifact,
  IntentReportArtifact,
  BundleArtifact,
  OracleReportArtifact,
  SubstrateQueryByPosition,
} from "./integration/interfaces.js";

export {
  isInvariantArtifact,
  isPatchArtifact,
  isRegressionTestArtifact,
  isPrincipleArtifact,
  isIntentReportArtifact,
  isBundleArtifact,
  isOracleReportArtifact,
} from "./integration/interfaces.js";

export {
  ReferenceTsMorphParser,
  ReferenceSqliteSubstrate,
  ReferenceGitWorktreeSandbox,
  ReferenceVitestPnpmRunner,
  ReferenceTypeScriptPatternScanner,
  ReferenceGitApplyApplicator,
  createReferenceImplementations,
} from "./integration/reference/index.js";

// ---------------------------------------------------------------------------
// Standing-runtime gate (no LLM)
// ---------------------------------------------------------------------------
export {
  verifyAll,
  resolveBindings,
  formatReport,
  exitCodeFor,
} from "./fix/runtime/verify.js";
export { verifyAllCached } from "./fix/runtime/verifyCache.js";

// ---------------------------------------------------------------------------
// Retrospective intake (B0)
// ---------------------------------------------------------------------------
export {
  extractIntent,
  IntentReportSchemaError,
  RetrospectiveIntakeInputError,
} from "./fix/intake/retrospective.js";

// ---------------------------------------------------------------------------
// Constraint store (invariant store)
// ---------------------------------------------------------------------------
export {
  readInvariants,
  writeInvariant,
  retireInvariant,
  buildStoredInvariant,
  hashInvariant,
  invariantStoreDir,
} from "./fix/runtime/invariantStore.js";

// ---------------------------------------------------------------------------
// LLM provider plumbing (so integrators can stub or wire their own)
// ---------------------------------------------------------------------------
export {
  StubLLMProvider,
  NotImplementedError,
  OverlayBypassError,
  InvariantFormulationFailed,
} from "./fix/types.js";

// ---------------------------------------------------------------------------
// Type surface
// ---------------------------------------------------------------------------
export type {
  StoredInvariant,
} from "./fix/runtime/invariantStore.js";

export type {
  InvariantClaim,
  BugSignal,
  BugLocus,
  TestArtifact,
  CodePatch,
  CodePatchFileEdit,
  FixCandidate,
  PrincipleCandidate,
  FixBundle,
  FixLoopResult,
  AuditEntry,
  LLMProvider,
  RemediationPlan,
  OverlayHandle,
  PlannedArtifact,
  ComplementaryChange,
  ComplementarySiteKind,
  CodeReference,
  InvariantCitation,
  BugProvenance,
  FixTemplate,
  TestTemplate,
  LibraryPrinciple,
  CapabilitySpec,
  ApplyResult,
  PrDraftArtifacts,
  RollbackAudit,
  StubAgentResponse,
} from "./fix/types.js";

export type {
  InvariantVerdict,
  VerifyReport,
  VerifyOptions,
  BindingResolution,
  DecayKind,
} from "./fix/runtime/verify.js";

export type {
  CachedVerifyReport,
  CachedInvariantVerdict,
  CacheEntry,
  VerifyEntry,
} from "./fix/runtime/verifyCache.js";

export type {
  IntentReport,
  IntentReportCitation,
  IntentReportIntent,
  IntentReportConstraintCandidate,
  IntentReportOutputBundle,
  IntentReportTrigger,
  RetrospectiveIntakeInput,
} from "./fix/intake/retrospective.js";

export type { Path, PathStep, EnumerateOptions } from "./fix/runtime/pathEnumerator.js";
export type { PathVerdict, CheckPathOptions } from "./fix/runtime/pathChecker.js";
