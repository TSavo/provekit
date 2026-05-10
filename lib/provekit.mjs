// provekit library entry: ESM shim.
//
// Statically re-exports every value name from the CJS shim so that
// `import { runFixLoop, ... } from "provekit"` works under Node's ESM
// loader. cjs-module-lexer cannot see through `module.exports = require(...)`
// in lib/provekit.cjs, so we cannot rely on `export * from "./provekit.cjs"`
// or named imports to flow through automatically. The destructure-then-re-export
// pattern below is what gives ESM consumers statically-bound names.
//
// Type-only exports (`export type` in src/index.ts) are erased at runtime
// and do NOT need to appear here. They are still importable via the
// "types" entry in package.json, which points at src/index.ts directly.

import lib from "./provekit.cjs";

export const {
  // Pipeline (full LLM-touching loop)
  runFixLoop,

  // Individual pipeline stages
  recognize,
  formulateInvariant,
  openOverlay,
  generateFixCandidate,
  generateComplementary,
  generateRegressionTest,
  generatePrincipleCandidate,
  assembleBundle,
  applyBundle,
  learnFromBundle,
  investigate,

  // Integration interface type guards
  isInvariantArtifact,
  isPatchArtifact,
  isRegressionTestArtifact,
  isPrincipleArtifact,
  isIntentReportArtifact,
  isBundleArtifact,
  isOracleReportArtifact,

  // Reference implementations of the channel-2 interfaces
  ReferenceTsMorphParser,
  ReferenceSqliteSubstrate,
  ReferenceGitWorktreeSandbox,
  ReferenceVitestPnpmRunner,
  ReferenceTypeScriptPatternScanner,
  ReferenceGitApplyApplicator,
  createReferenceImplementations,

  // Standing-runtime gate (no LLM)
  verifyAll,
  resolveBindings,
  formatReport,
  exitCodeFor,
  verifyAllCached,

  // Retrospective intake (B0)
  extractIntent,
  IntentReportSchemaError,
  RetrospectiveIntakeInputError,

  // Constraint store (invariant store)
  readInvariants,
  writeInvariant,
  retireInvariant,
  buildStoredInvariant,
  hashInvariant,
  invariantStoreDir,

  // LLM provider plumbing + error sentinels
  StubLLMProvider,
  NotImplementedError,
  OverlayBypassError,
  InvariantFormulationFailed,
} = lib;

export default lib;
