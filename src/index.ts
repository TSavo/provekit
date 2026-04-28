// Channel 2 — Library entry points for IDE / agent runtime / platform integration.
//
// This file is the SINGLE public entry point for the `provekit` package.
// Everything exported here is semver-committed: backward-compatibility on
// these names + signatures within the 0.x major. Anything NOT exported from
// this file is internal and may break without notice.
//
// The four canonical entry points named in
// docs/specs/2026-04-27-standing-invariant-runtime.md (Distribution surface,
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
export type { RunFixLoopArgs } from "./fix/orchestrator.js";

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
